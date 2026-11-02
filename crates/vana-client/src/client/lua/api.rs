//! The globals a mod sees (docs/lua-mods.md 4). Reads go through the world the host lends for the mod phase; writes
//! are queued as ops and applied after it, through the paths keys and text commands take.

use super::vm::{Handler, ModState, Timer};
use super::{Op, World};
use mlua::{Function, Lua, LuaSerdeExt, Table, Value, Variadic};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

/// The world, lent while the host runs a mod's Lua (null otherwise).
pub(super) struct WorldRef(pub *const World);

fn world(lua: &Lua) -> mlua::Result<&World> {
    let p = lua.app_data_ref::<WorldRef>().map_or(std::ptr::null(), |r| r.0);
    if p.is_null() {
        return Err(mlua::Error::runtime("game state is not available outside the mod phase"));
    }
    // The host sets the pointer from a live &World for the length of the call and clears it after.
    Ok(unsafe { &*p })
}

fn state(lua: &Lua) -> Rc<RefCell<ModState>> {
    lua.app_data_ref::<Rc<RefCell<ModState>>>().expect("vm state").clone()
}

fn next_id(st: &Rc<RefCell<ModState>>) -> u64 {
    let mut s = st.borrow_mut();
    s.next_id += 1;
    s.next_id
}

/// Every global, for the mod `vm`.
pub(super) fn install(lua: &Lua, vm: &super::vm::Vm, storage: (serde_json::Value, serde_json::Value), settings: serde_json::Value) -> mlua::Result<()> {
    let g = lua.globals();
    let m = &vm.manifest;

    let me = lua.create_table()?;
    me.set("id", m.id.as_str())?;
    me.set("name", m.name.as_str())?;
    me.set("version", m.version.as_str())?;
    me.set("api", super::manifest::API)?;
    me.set("path", vm.dir.to_string_lossy().as_ref())?;
    g.set("mod", me)?;

    // print and log: the chat log (dim) and vana-client.log.
    let id = m.id.clone();
    g.set(
        "print",
        lua.create_function(move |lua, args: Variadic<Value>| {
            let text = args.iter().map(|v| v.to_string().unwrap_or_else(|_| format!("{v:?}"))).collect::<Vec<_>>().join("\t");
            log::info!("lua {id}: {text}");
            state(lua).borrow_mut().ops.push(Op::Print(text));
            Ok(())
        })?,
    )?;
    let log = lua.create_table()?;
    for (name, level) in [("info", log::Level::Info), ("warn", log::Level::Warn), ("error", log::Level::Error), ("debug", log::Level::Debug)] {
        let id = m.id.clone();
        log.set(name, lua.create_function(move |_, text: String| {
            log::log!(level, "lua {id}: {text}");
            Ok(())
        })?)?;
    }
    g.set("log", log)?;

    // require: the mod's own files (a.b -> a/b.lua or a/b/init.lua), loaded once.
    let dir = vm.dir.clone();
    let loaded = lua.create_table()?;
    g.set(
        "require",
        lua.create_function(move |lua, name: String| {
            if let Ok(v) = loaded.raw_get::<Value>(name.as_str())
                && !v.is_nil()
            {
                return Ok(v);
            }
            if name.contains("..") || name.starts_with('/') {
                return Err(mlua::Error::runtime(format!("require {name:?}: outside the mod's folder")));
            }
            let rel = name.replace('.', "/");
            let path = [dir.join(format!("{rel}.lua")), dir.join(&rel).join("init.lua")].into_iter().find(|p| p.is_file()).ok_or_else(|| mlua::Error::runtime(format!("module {name:?} not found in the mod's folder")))?;
            let src = std::fs::read_to_string(&path).map_err(|e| mlua::Error::runtime(format!("{}: {e}", path.display())))?;
            let f = lua.load(src).set_name(format!("@{rel}.lua")).set_mode(mlua::chunk::ChunkMode::Text).into_function()?;
            let v: Value = f.call(name.as_str())?;
            let v = if v.is_nil() { Value::Boolean(true) } else { v };
            loaded.raw_set(name.as_str(), v.clone())?;
            Ok(v)
        })?,
    )?;

    // events.on(name, fn[, {priority}]) -> id; events.off(id); events.emit(name, ...) within this mod.
    let events = lua.create_table()?;
    events.set(
        "on",
        lua.create_function(|lua, (event, func, opts): (String, Function, Option<Table>)| {
            let st = state(lua);
            let id = next_id(&st);
            let priority = opts.and_then(|o| o.get::<i32>("priority").ok()).unwrap_or(0);
            let func = lua.create_registry_value(func)?;
            st.borrow_mut().handlers.push(Handler { id, event, func, priority });
            Ok(id)
        })?,
    )?;
    events.set(
        "off",
        lua.create_function(|lua, id: u64| {
            let st = state(lua);
            let removed: Vec<Handler> = {
                let mut s = st.borrow_mut();
                let (gone, keep): (Vec<_>, Vec<_>) = std::mem::take(&mut s.handlers).into_iter().partition(|h| h.id == id);
                s.handlers = keep;
                gone
            };
            for h in removed {
                lua.remove_registry_value(h.func)?;
            }
            Ok(())
        })?,
    )?;
    g.set("events", events)?;

    // commands.register(name, fn): //name and /name; commands.run(line): as if typed.
    let commands = lua.create_table()?;
    commands.set(
        "register",
        lua.create_function(|lua, (name, func): (String, Function)| {
            let name = name.trim_start_matches('/').to_ascii_lowercase();
            if name.is_empty() || name.contains(char::is_whitespace) {
                return Err(mlua::Error::runtime(format!("command name {name:?}")));
            }
            let st = state(lua);
            let key = lua.create_registry_value(func)?;
            let mut s = st.borrow_mut();
            s.commands.retain(|(n, _)| *n != name);
            s.commands.push((name, key));
            Ok(())
        })?,
    )?;
    commands.set(
        "run",
        lua.create_function(|lua, line: String| {
            state(lua).borrow_mut().ops.push(Op::Run(line));
            Ok(())
        })?,
    )?;
    g.set("commands", commands)?;

    // chat.add(text[, mode]): a log line in the mode's colour, like a game line.
    let chat = lua.create_table()?;
    chat.set(
        "add",
        lua.create_function(|lua, (text, mode): (String, Option<u8>)| {
            state(lua).borrow_mut().ops.push(Op::Chat { text, mode });
            Ok(())
        })?,
    )?;
    g.set("chat", chat)?;

    // timer.after(s, fn) / timer.every(s, fn) -> id; timer.cancel(id). clock(): monotonic seconds.
    let timer = lua.create_table()?;
    for (name, repeat) in [("after", false), ("every", true)] {
        timer.set(
            name,
            lua.create_function(move |lua, (secs, func): (f64, Function)| {
                let st = state(lua);
                let id = next_id(&st);
                let d = Duration::from_secs_f64(secs.max(if repeat { 0.01 } else { 0.0 }));
                let func = lua.create_registry_value(func)?;
                st.borrow_mut().timers.push(Timer { id, due: Instant::now() + d, every: repeat.then_some(d), func });
                Ok(id)
            })?,
        )?;
    }
    timer.set(
        "cancel",
        lua.create_function(|lua, id: u64| {
            let st = state(lua);
            let gone = {
                let mut s = st.borrow_mut();
                s.timers.iter().position(|t| t.id == id).map(|k| s.timers.remove(k))
            };
            if let Some(t) = gone {
                lua.remove_registry_value(t.func)?;
            }
            Ok(())
        })?,
    )?;
    g.set("timer", timer)?;
    let start = Instant::now();
    g.set("clock", lua.create_function(move |_, ()| Ok(start.elapsed().as_secs_f64()))?)?;

    // game: the façade's first areas (docs/lua-mods.md 4.6).
    let game = lua.create_table()?;
    game.set("player", lua.create_function(|lua, ()| player(lua))?)?;
    game.set("zone", lua.create_function(|lua, ()| Ok(world(lua)?.zone))?)?;
    game.set(
        "entity",
        lua.create_function(|lua, key: u32| {
            let w = world(lua)?;
            let e = w.entities.get(&key).map(|e| (key, e)).or_else(|| w.entities.iter().find(|(_, e)| u32::from(e.index) == key).map(|(id, e)| (*id, e)));
            e.map(|(id, e)| entity(lua, w, id, e)).transpose()
        })?,
    )?;
    game.set(
        "entities",
        lua.create_function(|lua, opts: Option<Table>| {
            let w = world(lua)?;
            let max = opts.as_ref().and_then(|o| o.get::<f32>("distance").ok());
            let kinds: Option<Vec<String>> = opts.as_ref().and_then(|o| o.get::<Vec<String>>("kinds").ok());
            let out = lua.create_table()?;
            let mut list: Vec<(&u32, &super::super::Entity)> = w.entities.iter().collect();
            list.sort_by_key(|(_, e)| e.index);
            for (id, e) in list {
                if max.is_some_and(|m| e.pos.distance(w.pos) > m) {
                    continue;
                }
                if kinds.as_ref().is_some_and(|k| !k.iter().any(|k| k == kind_name(e))) {
                    continue;
                }
                out.push(entity(lua, w, *id, e)?)?;
            }
            Ok(out)
        })?,
    )?;
    game.set(
        "target",
        lua.create_function(|lua, ()| {
            let w = world(lua)?;
            w.target.and_then(|id| w.entities.get(&id).map(|e| (id, e))).map(|(id, e)| entity(lua, w, id, e)).transpose()
        })?,
    )?;
    game.set("party", lua.create_function(|lua, ()| party(lua))?)?;
    game.set(
        "items",
        lua.create_function(|lua, bag: u8| {
            let w = world(lua)?;
            let out = lua.create_table()?;
            for s in w.inv.viewed(bag) {
                let (id, count) = w.inv.slots.get(&(bag, s.index)).copied().unwrap_or((0, 0));
                let t = lua.create_table()?;
                t.set("bag", bag)?;
                t.set("slot", s.index)?;
                t.set("id", id)?;
                t.set("count", count)?;
                if let Some(x) = w.inv.ext.get(&(bag, s.index)) {
                    t.set("extdata", lua.create_string(x)?)?;
                }
                t.set("locked", w.inv.locks.get(&(bag, s.index)).copied().unwrap_or(0))?;
                if let Some(p) = w.inv.prices.get(&(bag, s.index)) {
                    t.set("price", *p)?;
                }
                out.push(t)?;
            }
            Ok(out)
        })?,
    )?;
    game.set(
        "bag_size",
        lua.create_function(|lua, bag: u8| {
            let w = world(lua)?;
            Ok(w.inv.sizes.get(bag as usize).map(|s| s.saturating_sub(1)))
        })?,
    )?;
    game.set(
        "equipment",
        lua.create_function(|lua, ()| {
            let w = world(lua)?;
            let out = lua.create_table()?;
            for (slot, at) in w.inv.equip.iter().enumerate() {
                if let Some((bag, index)) = at {
                    let t = lua.create_table()?;
                    t.set("bag", *bag)?;
                    t.set("slot", *index)?;
                    t.set("id", w.inv.item(*bag, *index))?;
                    out.set(slot, t)?;
                }
            }
            Ok(out)
        })?,
    )?;
    game.set(
        "buffs",
        lua.create_function(|lua, ()| {
            let w = world(lua)?;
            let now = (super::super::vana_now(w) as u32).wrapping_mul(60);
            let out = lua.create_table()?;
            for (id, end) in super::super::buffs::icons(w) {
                let t = lua.create_table()?;
                t.set("id", id)?;
                if end != 0x7fff_ffff {
                    t.set("remaining", (end.wrapping_sub(now) as i32).max(0) as f64 / 60.0)?;
                }
                out.push(t)?;
            }
            Ok(out)
        })?,
    )?;
    game.set(
        "recasts",
        lua.create_function(|lua, ()| {
            let w = world(lua)?;
            let now = Instant::now();
            let left = |at: &Instant| at.saturating_duration_since(now).as_secs_f64();
            let spells = lua.create_table()?;
            for (id, at) in &w.spell_recast {
                spells.set(*id, left(at))?;
            }
            let abilities = lua.create_table()?;
            for (id, at) in &w.ability_recast {
                abilities.set(*id, left(at))?;
            }
            let t = lua.create_table()?;
            t.set("spells", spells)?;
            t.set("abilities", abilities)?;
            Ok(t)
        })?,
    )?;
    game.set(
        "time",
        lua.create_function(|lua, ()| {
            let w = world(lua)?;
            let earth = super::super::vana_now(w);
            let p = crate::dat::msgfmt::VanaParts::of(earth);
            let t = lua.create_table()?;
            t.set("earth", earth)?;
            t.set("year", p.year)?;
            t.set("month", p.month)?;
            t.set("day", p.day)?;
            t.set("hour", p.hour)?;
            t.set("minute", p.minute)?;
            t.set("weekday", p.weekday)?;
            t.set("moon_phase", p.phase)?;
            t.set("moon", p.moon)?;
            Ok(t)
        })?,
    )?;
    game.set("weather", lua.create_function(|lua, ()| Ok(world(lua)?.weather))?)?;
    game.set("key_items", lua.create_function(|lua, ()| Ok(world(lua)?.key_items.clone()))?)?;
    g.set("game", game)?;

    // storage.account / storage.character: plain data, saved at logout, unload and storage.save().
    let st_table = lua.create_table()?;
    st_table.set("account", json_table(lua, &storage.0)?)?;
    st_table.set("character", json_table(lua, &storage.1)?)?;
    st_table.set(
        "save",
        lua.create_function(|lua, ()| {
            state(lua).borrow_mut().save = true;
            Ok(())
        })?,
    )?;
    g.set("storage", st_table)?;

    // settings.get(key): the user's value, else the declared default.
    let decl = m.settings.clone();
    let settings_table = lua.create_table()?;
    settings_table.set(
        "get",
        lua.create_function(move |lua, key: String| {
            if let Some(v) = settings.get(&key) {
                return lua.to_value(v);
            }
            match decl.get(&key).and_then(|s| s.default.as_ref()) {
                Some(v) => lua.to_value(v),
                None => Err(mlua::Error::runtime(format!("no setting {key:?} in mod.toml"))),
            }
        })?,
    )?;
    g.set("settings", settings_table)?;
    Ok(())
}

/// A stored JSON value as a Lua table (an empty table for nothing stored).
fn json_table(lua: &Lua, v: &serde_json::Value) -> mlua::Result<Value> {
    match v {
        serde_json::Value::Object(o) if !o.is_empty() => lua.to_value(v),
        _ => Ok(Value::Table(lua.create_table()?)),
    }
}

fn kind_name(e: &super::super::Entity) -> &'static str {
    match (e.kind, e.mob) {
        (super::super::net::EntityKind::Player, _) => "player",
        (_, true) => "mob",
        _ => "npc",
    }
}

/// An entity as a table: who, where, how it stands.
fn entity(lua: &Lua, w: &World, id: u32, e: &super::super::Entity) -> mlua::Result<Table> {
    let t = lua.create_table()?;
    t.set("id", id)?;
    t.set("index", e.index)?;
    t.set("name", e.name.as_str())?;
    t.set("kind", kind_name(e))?;
    let pos = lua.create_table()?;
    pos.set("x", e.pos.x)?;
    pos.set("y", e.pos.y)?;
    pos.set("z", e.pos.z)?;
    t.set("pos", pos)?;
    t.set("heading", e.yaw)?;
    t.set("distance", e.pos.distance(w.pos))?;
    t.set("hpp", e.hpp)?;
    t.set("status", e.motion.status)?;
    t.set("speed", f32::from(e.motion.speed) / 10.0)?;
    t.set("claim", e.claim)?;
    t.set("targetable", e.flags & 6 == 0 && !e.untargetable && !e.target_off)?;
    t.set("hidden", e.flags & 2 != 0)?;
    Ok(t)
}

/// game.party(): every member of the party and alliance, by slot (0..5 our party, 6..17 the others).
fn party(lua: &Lua) -> mlua::Result<Table> {
    let w = world(lua)?;
    let s = w.party.slots();
    let out = lua.create_table()?;
    for (slot, m) in s.slots.iter().enumerate() {
        let Some(m) = m else { continue };
        let t = lua.create_table()?;
        t.set("slot", slot)?;
        t.set("party", slot / 6)?;
        t.set("id", m.id)?;
        t.set("index", m.index)?;
        t.set("name", m.name.as_str())?;
        t.set("hp", m.hp)?;
        t.set("mp", m.mp)?;
        t.set("tp", m.tp)?;
        t.set("hpp", m.hpp)?;
        t.set("mpp", m.mpp)?;
        t.set("zone", m.zone)?;
        t.set("in_zone", m.away == 0)?;
        t.set("main_job", m.jobs[0])?;
        t.set("main_level", m.jobs[1])?;
        t.set("sub_job", m.jobs[2])?;
        t.set("sub_level", m.jobs[3])?;
        t.set("leader", s.leaders.get(slot / 6).is_some_and(|l| *l == m.id))?;
        t.set("alliance_leader", s.alliance_leader == m.id)?;
        let buffs: Vec<u16> = w.party_buffs.get(&m.id).map(|b| b.iter().copied().filter(|&b| b != 0xffff && b != 0).collect()).unwrap_or_default();
        t.set("buffs", buffs)?;
        out.push(t)?;
    }
    Ok(out)
}

/// game.player(): who and where we are, vitals and jobs.
fn player(lua: &Lua) -> mlua::Result<Table> {
    let w = world(lua)?;
    let t = lua.create_table()?;
    t.set("name", w.name.as_str())?;
    t.set("id", w.my_id)?;
    t.set("index", w.my_index)?;
    t.set("zone", w.zone)?;
    let pos = lua.create_table()?;
    pos.set("x", w.pos.x)?;
    pos.set("y", w.pos.y)?;
    pos.set("z", w.pos.z)?;
    t.set("pos", pos)?;
    t.set("heading", w.yaw)?;
    t.set("status", w.my_motion.status)?;
    if let Some((hp, mp, tp, hpp, mpp)) = w.vitals {
        t.set("hp", hp)?;
        t.set("mp", mp)?;
        t.set("tp", tp)?;
        t.set("hpp", hpp)?;
        t.set("mpp", mpp)?;
    }
    if let Some(s) = &w.stats {
        t.set("hp_max", s.hp_max)?;
        t.set("mp_max", s.mp_max)?;
        t.set("main_job", s.main.0)?;
        t.set("main_level", s.main.1)?;
        t.set("sub_job", s.sub.0)?;
        t.set("sub_level", s.sub.1)?;
    }
    t.set("target", w.target)?;
    Ok(t)
}
