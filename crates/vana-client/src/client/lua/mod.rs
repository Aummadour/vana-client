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
        Ok(())
    }

    /// Sends `unload`, saves its storage and drops the VM.
    fn unload(&mut self, w: &World, k: usize) {
        let vm = self.vms.remove(k);
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
