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
