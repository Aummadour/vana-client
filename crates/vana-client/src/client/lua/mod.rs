//! Lua mods (docs/lua-mods.md): a VM per enabled mod with a mod.toml in its folder under mods/, created when the
//! character enters the world and dropped at logout. Once a frame the mod phase delivers events, commands, timers and
//! wake-ups; what the mods asked for is applied after it.

mod api;
mod docs;
mod manifest;
mod vm;

use super::World;
use std::path::PathBuf;
use vm::Vm;

/// What a mod asked the client to do, applied after the mod phase.
pub(super) enum Op {
    Chat { text: String, mode: Option<u8> },
    Print(String),
    Run(String),
}

/// The mods of this session.
#[derive(Default)]
pub struct Host {
    vms: Vec<Vm>,
    started: bool,
    /// Events raised by the client since the last mod phase.
    events: Vec<(&'static str, Vec<serde_json::Value>)>,
    /// Lines typed for mods (a registered command, or /lua), run in the next mod phase.
    typed: Vec<String>,
    last_frame: Option<std::time::Instant>,
    /// Last frame's state, for the derived change events.
    prev: Option<Snap>,
    /// The chat filter must be rebuilt (a mod loaded, unloaded or changed its filters).
    filters_dirty: bool,
}

/// What the derived events watch (Windower's change events).
#[derive(Clone, Default, PartialEq)]
struct Snap {
    vitals: Option<(u32, u32, u32, u8, u8)>,
    status: u8,
    jobs: Option<(u8, u8, u8, u8)>,
    buffs: Vec<u16>,
    target: Option<u32>,
    minute: Option<i32>,
    weekday: Option<usize>,
    moon: Option<usize>,
    weather: u16,
    party: Vec<u32>,
    items: Option<std::collections::BTreeMap<(u8, u8), (u16, u32)>>,
}

impl Snap {
    fn of(w: &World, items: bool) -> Self {
        let clock = w.clock.map(|_| crate::dat::msgfmt::VanaParts::of(super::vana_now(w)));
        let mut buffs: Vec<u16> = w.buffs.iter().copied().filter(|&b| b != 0xffff).collect();
        buffs.sort_unstable();
        Self {
            vitals: w.vitals,
            status: w.my_motion.status,
            jobs: w.stats.as_ref().map(|s| (s.main.0, s.main.1, s.sub.0, s.sub.1)),
            buffs,
            target: w.target,
            minute: clock.as_ref().map(|p| p.hour * 60 + p.minute),
            weekday: clock.as_ref().map(|p| p.weekday),
            moon: clock.as_ref().map(|p| p.phase),
            weather: w.weather,
            party: w.party.slots().slots.iter().flatten().map(|m| m.id).collect(),
            items: items.then(|| w.inv.slots.clone()),
        }
    }
}

/// The change events between two snapshots: (name, arguments).
fn changes(a: &Snap, b: &Snap) -> Vec<(&'static str, Vec<serde_json::Value>)> {
    use serde_json::Value as J;
    let mut out = Vec::new();
    if let (Some(o), Some(n)) = (a.vitals, b.vitals) {
        for (name, nv, ov) in [("hp_change", n.0, o.0), ("mp_change", n.1, o.1), ("tp_change", n.2, o.2), ("hpp_change", n.3 as u32, o.3 as u32), ("mpp_change", n.4 as u32, o.4 as u32)] {
            if nv != ov {
                out.push((name, vec![nv.into(), ov.into()]));
            }
        }
    }
    if a.status != b.status {
        out.push(("status_change", vec![b.status.into(), a.status.into()]));
    }
    if let (Some(o), Some(n)) = (a.jobs, b.jobs) {
        if o.0 != n.0 || o.2 != n.2 {
            out.push(("job_change", vec![n.0.into(), n.1.into(), n.2.into(), n.3.into()]));
        } else if n.1 > o.1 {
            out.push(("level_up", vec![n.1.into()]));
        } else if n.1 < o.1 {
            out.push(("level_down", vec![n.1.into()]));
        }
    }
    // Buffs as multisets: each id gained or lost once per copy.
    let mut lost = a.buffs.clone();
    for id in &b.buffs {
        match lost.iter().position(|x| x == id) {
            Some(k) => {
                lost.remove(k);
            }
            None => out.push(("buff_gain", vec![(*id).into()])),
        }
    }
    for id in lost {
        out.push(("buff_lose", vec![id.into()]));
    }
    if a.target != b.target {
        out.push(("target_change", vec![b.target.map_or(J::Null, J::from), a.target.map_or(J::Null, J::from)]));
    }
    if let (Some(o), Some(n)) = (a.minute, b.minute)
        && o != n
    {
        out.push(("time_change", vec![n.into(), o.into()]));
    }
    if let (Some(o), Some(n)) = (a.weekday, b.weekday)
        && o != n
    {
        out.push(("day_change", vec![n.into(), o.into()]));
    }
    if let (Some(o), Some(n)) = (a.moon, b.moon)
        && o != n
    {
        out.push(("moon_change", vec![n.into(), o.into()]));
    }
    if a.weather != b.weather {
        out.push(("weather_change", vec![b.weather.into(), a.weather.into()]));
    }
    if a.party != b.party {
        out.push(("party_change", vec![]));
    }
    if let (Some(o), Some(n)) = (&a.items, &b.items) {
        for (&(bag, slot), &(id, count)) in n {
            match o.get(&(bag, slot)) {
                Some(&(oid, ocount)) if oid == id && ocount == count => {}
                Some(&(oid, ocount)) if oid == id => out.push(("item_change", vec![bag.into(), slot.into(), id.into(), count.into(), ocount.into()])),
                Some(&(oid, ocount)) => {
                    out.push(("item_remove", vec![bag.into(), slot.into(), oid.into(), ocount.into()]));
                    out.push(("item_add", vec![bag.into(), slot.into(), id.into(), count.into()]));
                }
                None => out.push(("item_add", vec![bag.into(), slot.into(), id.into(), count.into()])),
            }
        }
        for (&(bag, slot), &(id, count)) in o {
            if !n.contains_key(&(bag, slot)) {
                out.push(("item_remove", vec![bag.into(), slot.into(), id.into(), count.into()]));
            }
        }
    }
    out
}

/// Where mods keep their settings and storage: beside an FFXI_CONFIG copy for test runs.
fn data_dir() -> PathBuf {
    match std::env::var_os("FFXI_CONFIG") {
        Some(p) => PathBuf::from(p).with_file_name("mod-data"),
        None => crate::config::data_dir().join("mod-data"),
    }
}

/// The mods' folders; a test run's own (`mods` beside its FFXI_CONFIG copy) when it has one, so test mods never sit
/// among the user's.
fn mods_dir() -> PathBuf {
    match std::env::var_os("FFXI_CONFIG").map(|p| PathBuf::from(p).with_file_name("mods")) {
        Some(d) if d.is_dir() => d,
        _ => crate::config::mods_dir(),
    }
}

fn read_json(path: &std::path::Path) -> serde_json::Value {
    std::fs::read(path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or(serde_json::Value::Null)
}

impl Host {
    /// A client event for the mods (delivered in the next mod phase).
    pub fn raise(&mut self, name: &'static str, args: Vec<serde_json::Value>) {
        if self.started {
            self.events.push((name, args));
        }
    }

    /// Whether a typed line is for the mods: //anything, /lua, or a mod's registered /command.
    pub fn takes(&self, line: &str) -> bool {
        if line.starts_with("//") {
            return true;
        }
        let first = line.trim_start_matches('/').split_whitespace().next().unwrap_or("").to_ascii_lowercase();
        first == "lua" || self.vms.iter().any(|v| v.suspended.is_none() && v.st.borrow().commands.iter().any(|(n, _)| *n == first))
    }

    pub fn type_line(&mut self, line: &str) {
        self.typed.push(line.to_string());
    }

    fn char_file(w: &World) -> String {
        let clean = |s: &str| s.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>();
        format!("character-{}-{}.json", clean(&w.world_name), clean(&w.name))
    }

    /// Creates the VM of the mod in `dir`, runs its entry file and sends it `load`.
    fn load(&mut self, w: &World, dir: PathBuf) -> Result<(), String> {
        let m = manifest::read(&dir).map_err(|e| format!("{e:#}"))?;
        if self.vms.iter().any(|v| v.manifest.id == m.id) {
            return Err(format!("{} is already loaded", m.id));
        }
        let data = data_dir().join(&m.id);
        let storage = (read_json(&data.join("account.json")), read_json(&data.join(Self::char_file(w))));
        let settings = read_json(&data.join("settings.json"));
        let vm = Vm::new(m, dir).map_err(|e| format!("{e:#}"))?;
        api::install(&vm.lua, &vm, storage, settings).map_err(|e| format!("{e}"))?;
        log::info!("lua mod {} {} loaded from {}", vm.manifest.id, vm.manifest.version, vm.dir.display());
        lend(&vm, w, |vm| {
            vm.run_entry();
            vm.emit("load", ());
        });
        self.vms.push(vm);
        self.filters_dirty = true;
        Ok(())
    }

    /// Sends `unload`, saves its storage and drops the VM.
    fn unload(&mut self, w: &World, k: usize) {
        let vm = self.vms.remove(k);
        self.filters_dirty = true;
        lend(&vm, w, |vm| vm.emit("unload", ()));
        save(&vm, w);
        log::info!("lua mod {} unloaded", vm.manifest.id);
    }

    /// The enabled mods (config `mods`) whose folders have a mod.toml.
    fn start(&mut self, w: &mut World) {
        self.started = true;
        let cfg = crate::config::Config::load();
        let root = mods_dir();
        for r in cfg.mods.iter().filter(|r| r.enabled) {
            let dir = root.join(&r.name);
            if !dir.join("mod.toml").is_file() {
                continue;
            }
            if let Err(e) = self.load(w, dir) {
                log::warn!("lua mod {}: {e}", r.name);
                w.chat.push_colour(format!("Mod {}: {e}", r.name), ERROR);
            }
        }
        let name = w.name.clone();
        self.events.push(("login", vec![name.into()]));
    }

    /// Logout: `logout` and `unload` to every mod, storage saved.
    pub fn stop(&mut self, w: &World) {
        for vm in &self.vms {
            lend(vm, w, |vm| vm.emit("logout", ()));
        }
        while !self.vms.is_empty() {
            self.unload(w, 0);
        }
        self.started = false;
        self.prev = None;
    }

    /// `/lua ...`: list, load, unload, reload, run, set.
    fn lua_command(&mut self, w: &World, args: &[&str], out: &mut Vec<(String, [u8; 4])>) {
        let find = |vms: &[Vm], id: &str| vms.iter().position(|v| v.manifest.id.eq_ignore_ascii_case(id));
        match args.first().copied().unwrap_or("list") {
            "list" => {
                if self.vms.is_empty() {
                    out.push(("No Lua mods loaded.".into(), super::WHITE));
                }
                for v in &self.vms {
                    let st = v.st.borrow();
                    let state = v.suspended.as_deref().map_or("running".to_string(), |r| format!("suspended: {r}"));
                    out.push((format!("{} {} ({state}): {:.2} ms avg, {:.1} ms peak, {} KB", v.manifest.id, v.manifest.version, st.stats.avg, st.stats.peak, v.lua.used_memory() / 1024), super::WHITE));
                }
            }
            "load" => match args.get(1) {
                Some(id) => {
                    let dir = mods_dir().join(id);
                    match self.load(w, dir) {
                        Ok(()) => out.push((format!("Loaded {id}."), super::WHITE)),
                        Err(e) => out.push((e, ERROR)),
                    }
                }
                None => out.push(("/lua load <mod>".into(), ERROR)),
            },
            "unload" | "reload" => match args.get(1).and_then(|id| find(&self.vms, id)) {
                Some(k) => {
                    let dir = self.vms[k].dir.clone();
                    let id = self.vms[k].manifest.id.clone();
                    self.unload(w, k);
                    if args[0] == "reload" {
                        match self.load(w, dir) {
                            Ok(()) => out.push((format!("Reloaded {id}."), super::WHITE)),
                            Err(e) => out.push((e, ERROR)),
                        }
                    } else {
                        out.push((format!("Unloaded {id}."), super::WHITE));
                    }
                }
                None => out.push(("No such mod loaded.".into(), ERROR)),
            },
            "run" => match args.get(1).and_then(|id| find(&self.vms, id)) {
                Some(k) => {
                    let code = args[2..].join(" ");
                    let vm = &self.vms[k];
                    lend(vm, w, |vm| match vm.lua.load(code).set_name("=/lua run").into_function() {
                        Ok(f) => vm.call(f, ()),
                        Err(e) => vm.error(&e),
                    });
                }
                None => out.push(("/lua run <mod> <code>".into(), ERROR)),
            },
            "set" => match (args.get(1), args.get(2), args.get(3)) {
                (Some(id), Some(key), Some(_)) => {
                    let path = data_dir().join(id).join("settings.json");
                    let mut v = read_json(&path);
                    if !v.is_object() {
                        v = serde_json::json!({});
                    }
                    let raw = args[3..].join(" ");
                    let value = serde_json::from_str(&raw).unwrap_or(serde_json::Value::String(raw));
                    v[*key] = value;
                    let _ = std::fs::create_dir_all(path.parent().unwrap());
                    let _ = std::fs::write(&path, serde_json::to_vec_pretty(&v).unwrap_or_default());
                    out.push((format!("{id}: {key} set (reload the mod to apply)."), super::WHITE));
                }
                _ => out.push(("/lua set <mod> <key> <value>".into(), ERROR)),
            },
            other => out.push((format!("/lua {other}: list, load, unload, reload, run, set"), ERROR)),
        }
    }
}

/// An error line's colour (BGRA, 0x80 = 1.0: the log's red).
const ERROR: [u8; 4] = [0x30, 0x30, 0x80, 0x80];
/// A mod's print line (dim).
const DIM: [u8; 4] = [0x60, 0x60, 0x60, 0x80];

/// Lends the world to `vm` for `f`.
fn lend(vm: &Vm, w: &World, f: impl FnOnce(&Vm)) {
    vm.lua.set_app_data(api::WorldRef(w as *const World));
    f(vm);
    vm.lua.remove_app_data::<api::WorldRef>();
}

/// Writes a mod's storage tables.
fn save(vm: &Vm, w: &World) {
    let dir = data_dir().join(&vm.manifest.id);
    let Ok(storage) = vm.lua.globals().get::<mlua::Table>("storage") else { return };
    for (key, file) in [("account", "account.json".to_string()), ("character", Host::char_file(w))] {
        let Ok(t) = storage.get::<mlua::Value>(key) else { continue };
        use mlua::LuaSerdeExt;
        match vm.lua.from_value::<serde_json::Value>(t) {
            Ok(v) => {
                let _ = std::fs::create_dir_all(&dir);
                if let Err(e) = std::fs::write(dir.join(&file), serde_json::to_vec_pretty(&v).unwrap_or_default()) {
                    log::warn!("lua mod {}: saving {file}: {e}", vm.manifest.id);
                }
            }
            Err(e) => log::warn!("lua mod {}: storage.{key} is not plain data: {e}", vm.manifest.id),
        }
    }
}

/// The mod phase (docs/lua-mods.md 3.2): once a frame in the world.
pub(super) fn frame(w: &mut World) {
    let mut host = std::mem::take(&mut w.lua);
    if !host.started {
        if w.zone.is_none() || w.name.is_empty() {
            w.lua = host;
            return;
        }
        host.start(w);
    }
    // The derived change events (items only while someone listens: the bags are big).
    let items = host.vms.iter().any(|v| ["item_add", "item_remove", "item_change"].iter().any(|e| v.has_handler(e)));
    let snap = Snap::of(w, items);
    if let Some(prev) = &host.prev {
        let found = changes(prev, &snap);
        host.events.extend(found);
    }
    host.prev = Some(snap);
    let now = std::time::Instant::now();
    let dt = host.last_frame.map_or(0.0, |t| now.duration_since(t).as_secs_f64());
    host.last_frame = Some(now);
    let mut out: Vec<(String, [u8; 4])> = Vec::new();
    {
        let w: &World = w;
        // Lines typed for mods.
        for line in std::mem::take(&mut host.typed) {
            let body = line.trim_start_matches('/');
            let mut words = body.split_whitespace();
            let name = words.next().unwrap_or("").to_ascii_lowercase();
            let args: Vec<&str> = words.collect();
            if name == "lua" {
                host.lua_command(w, &args, &mut out);
                continue;
            }
            let owner = host.vms.iter().find(|v| v.suspended.is_none() && v.st.borrow().commands.iter().any(|(n, _)| *n == name));
            match owner {
                Some(vm) => lend(vm, w, |vm| {
                    let f = vm.st.borrow().commands.iter().find(|(n, _)| *n == name).and_then(|(_, k)| vm.lua.registry_value::<mlua::Function>(k).ok());
                    if let Some(f) = f {
                        let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
                        vm.call(f, mlua::Variadic::from_iter(args));
                    }
                }),
                None => out.push((format!("No mod command {name}."), ERROR)),
            }
        }
        let events = std::mem::take(&mut host.events);
        for vm in host.vms.iter().filter(|v| v.suspended.is_none()) {
            lend(vm, w, |vm| {
                use mlua::LuaSerdeExt;
                for (name, args) in &events {
                    if !vm.has_handler(name) {
                        continue;
                    }
                    let vals: Vec<mlua::Value> = args.iter().map(|a| vm.lua.to_value(a).unwrap_or(mlua::Value::Nil)).collect();
                    vm.emit(name, mlua::MultiValue::from_iter(vals));
                }
                vm.timers();
                vm.wake();
                if vm.has_handler("frame") {
                    vm.emit("frame", dt);
                }
            });
        }
    }
    // Accounting, failures, saves and the mods' ops.
    let mut ops = Vec::new();
    for vm in host.vms.iter_mut() {
        vm.end_frame();
        if vm.suspended.is_none()
            && let Some(why) = vm.failing()
        {
            out.push((format!("Mod {} suspended: {why}. /lua reload {} to start it again.", vm.manifest.name, vm.manifest.id), ERROR));
            vm.suspended = Some(why);
        }
        let (notices, vm_ops, save_now) = {
            let mut st = vm.st.borrow_mut();
            (std::mem::take(&mut st.notices), std::mem::take(&mut st.ops), std::mem::take(&mut st.save))
        };
        out.extend(notices.into_iter().map(|n| (n, ERROR)));
        ops.extend(vm_ops);
        if save_now {
            save(vm, w);
        }
    }
    let dirty = std::mem::take(&mut host.filters_dirty) | host.vms.iter().fold(false, |d, v| d | std::mem::take(&mut v.st.borrow_mut().filters_dirty));
    if dirty {
        w.chat.filter = host.chat_filter();
        w.chat.mode_colours = w.battle.as_ref().map(|b| std::rc::Rc::new(b.code_tables(&w.game).1));
    }
    w.lua = host;
    for (line, colour) in out {
        w.chat.push_colour(line, colour);
    }
    for op in ops {
        apply(w, op);
    }
}

fn apply(w: &mut World, op: Op) {
    match op {
        Op::Print(text) => w.chat.push_colour(text, DIM),
        Op::Chat { text, mode } => match mode {
            Some(m) => {
                let (colour, indent) = w.battle.as_ref().map_or((super::WHITE, false), |b| (b.colour_in(m, &w.game), b.indents(m)));
                w.chat.push_event(text, colour, indent, m);
            }
            None => w.chat.push(text),
        },
        Op::Run(line) => super::textcmd::run(w, &line, super::textcmd::Mode::Typed),
    }
}

/// LuaLS stubs for the whole mod API (`vana-client lua-stubs`).
pub fn stubs() -> String {
    docs::stubs()
}

#[cfg(test)]
mod tests {
    use super::*;
    use mlua::{Table, Value};

    /// Every documented function is bound, and every function in a documented namespace is documented.
    #[test]
    fn docs_match_bindings() {
        let vm = Vm::new(manifest::Manifest::default(), PathBuf::from(".")).unwrap();
        api::install(&vm.lua, &vm, (serde_json::Value::Null, serde_json::Value::Null), serde_json::Value::Null).unwrap();
        let g = vm.lua.globals();
        for f in docs::FUNCS {
            let v: Value = if f.ns.is_empty() { g.get(f.name).unwrap() } else { g.get::<Table>(f.ns).unwrap().get(f.name).unwrap() };
            assert!(matches!(v, Value::Function(_)), "{}.{} is documented but not bound", f.ns, f.name);
        }
        let namespaces: std::collections::BTreeSet<&str> = docs::FUNCS.iter().map(|f| f.ns).filter(|n| !n.is_empty()).collect();
        for ns in namespaces {
            let t: Table = g.get(ns).unwrap();
            for pair in t.pairs::<String, Value>() {
                let (name, v) = pair.unwrap();
                if matches!(v, Value::Function(_)) {
                    assert!(docs::FUNCS.iter().any(|f| f.ns == ns && f.name == name), "{ns}.{name} is bound but not documented");
                }
            }
        }
    }
}

/// A decoded s2c 0x028 for the `action` event (Windower's action table, our field names).
pub(super) fn action_json(a: &crate::net::action::Action) -> serde_json::Value {
    use serde_json::json;
    let side = |s: &Option<crate::net::action::Side>| s.as_ref().map(|s| json!({"kind": s.kind, "info": s.info, "value": s.value, "message": s.message}));
    let targets: Vec<serde_json::Value> = a
        .targets
        .iter()
        .map(|t| {
            let results: Vec<serde_json::Value> = t
                .results
                .iter()
                .map(|r| json!({"miss": r.miss, "kind": r.kind, "animation": r.animation, "info": r.info, "scale": r.scale, "value": r.value, "message": r.message, "bit": r.bit, "proc": side(&r.proc), "react": side(&r.react)}))
                .collect();
            json!({"id": t.id, "results": results})
        })
        .collect();
    json!({"actor": a.actor, "category": a.category, "param": a.arg, "info": a.info, "targets": targets})
}

impl Host {
    /// The chat filter over every running mod's `filters.chat` functions, in load order; none when there are none.
    fn chat_filter(&self) -> Option<std::rc::Rc<dyn Fn(&str, u16) -> super::combat::Filtered>> {
        type Entry = (String, mlua::Lua, mlua::Function, std::rc::Rc<std::cell::RefCell<vm::ModState>>);
        let mut list: Vec<Entry> = Vec::new();
        for vm in self.vms.iter().filter(|v| v.suspended.is_none()) {
            for (_, k) in &vm.st.borrow().chat_filters {
                if let Ok(f) = vm.lua.registry_value::<mlua::Function>(k) {
                    list.push((vm.manifest.name.clone(), vm.lua.clone(), f, vm.st.clone()));
                }
            }
        }
        if list.is_empty() {
            return None;
        }
        Some(std::rc::Rc::new(move |text: &str, mode: u16| {
            use super::combat::{Filtered, NO_MODE, is_battle};
            let (mut text, mut mode, mut changed) = (text.to_string(), mode, false);
            for (name, lua, f, st) in &list {
                let game_mode = (mode != NO_MODE && mode != super::combat::BATTLE_MODE).then_some(mode as u8);
                st.borrow_mut().deadline = Some(std::time::Instant::now() + vm::FILTER_CAP);
                let r = f.call::<mlua::MultiValue>((text.as_str(), game_mode, is_battle(mode)));
                st.borrow_mut().deadline = None;
                let _ = lua;
                match r {
                    Err(e) => {
                        log::warn!("lua mod {name}: chat filter: {e}");
                        st.borrow_mut().record(name, &e);
                    }
                    Ok(v) => match v.front() {
                        Some(mlua::Value::Boolean(false)) => return Filtered::Block,
                        Some(mlua::Value::String(t)) => {
                            text = t.to_string_lossy();
                            if let Some(m) = v.get(1).and_then(|m| m.as_integer()) {
                                mode = if is_battle(mode) { super::combat::BATTLE | (m as u16 & 0xff) } else { m as u16 & 0xff };
                            }
                            changed = true;
                        }
                        _ => {}
                    },
                }
            }
            if changed { Filtered::Replace(text, mode) } else { Filtered::Keep }
        }))
    }
}

/// The mods' typed-line filters (docs/lua-mods.md 4.3), before our command parser: the line as they left it, or none
/// when one blocked it.
pub(super) fn filter_typed(w: &mut World, line: &str) -> Option<String> {
    if w.lua.vms.iter().all(|v| v.st.borrow().typed_filters.is_empty()) {
        return Some(line.to_string());
    }
    let host = std::mem::take(&mut w.lua);
    let mut line = line.to_string();
    let mut blocked = false;
    for vm in host.vms.iter().filter(|v| v.suspended.is_none()) {
        let funcs: Vec<mlua::Function> = vm.st.borrow().typed_filters.iter().filter_map(|(_, k)| vm.lua.registry_value(k).ok()).collect();
        lend(vm, w, |vm| {
            for f in funcs {
                if blocked {
                    return;
                }
                vm.st.borrow_mut().deadline = Some(std::time::Instant::now() + vm::FILTER_CAP);
                let r = f.call::<mlua::Value>(line.as_str());
                vm.st.borrow_mut().deadline = None;
                match r {
                    Err(e) => vm.error(&e),
                    Ok(mlua::Value::Boolean(false)) => blocked = true,
                    Ok(mlua::Value::String(t)) => line = t.to_string_lossy(),
                    Ok(_) => {}
                }
            }
        });
        if blocked {
            break;
        }
    }
    w.lua = host;
    (!blocked).then_some(line)
}
