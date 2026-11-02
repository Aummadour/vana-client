//! The mod API's reference (docs/lua-mods.md 4): one entry per global function and event. `lua-stubs` writes LuaLS
//! stubs from it, and a test keeps it equal to what api.rs binds.

pub struct Func {
    /// The namespace table, or "" for a global function.
    pub ns: &'static str,
    pub name: &'static str,
    pub doc: &'static str,
    /// (name, LuaLS type, doc); a name ending in '?' is optional.
    pub params: &'static [(&'static str, &'static str, &'static str)],
    /// The LuaLS return type ("" for none).
    pub ret: &'static str,
}

pub struct Event {
    pub name: &'static str,
    pub doc: &'static str,
    pub args: &'static [(&'static str, &'static str)],
}

pub const FUNCS: &[Func] = &[
    Func { ns: "", name: "print", doc: "Writes its arguments to the chat log (dim) and vana-client.log.", params: &[("...", "any", "values, tab-separated")], ret: "" },
    Func { ns: "", name: "require", doc: "Loads a Lua file from the mod's folder once (a.b is a/b.lua or a/b/init.lua) and returns what it returned.", params: &[("name", "string", "module name")], ret: "any" },
    Func { ns: "", name: "sleep", doc: "Pauses the running handler for `seconds` (other handlers and the game go on).", params: &[("seconds?", "number", "default 0: the next frame")], ret: "" },
    Func { ns: "", name: "sleep_frames", doc: "Pauses the running handler for `n` frames.", params: &[("n?", "integer", "default 1")], ret: "" },
    Func { ns: "", name: "wait_until", doc: "Pauses the running handler until `test()` returns true (checked once a frame), or `timeout` seconds pass.", params: &[("test", "fun(): boolean", "the condition"), ("timeout?", "number", "seconds; none waits for ever")], ret: "" },
    Func { ns: "", name: "clock", doc: "Seconds since the mod was loaded (monotonic).", params: &[], ret: "number" },
    Func { ns: "log", name: "info", doc: "Writes a line to vana-client.log.", params: &[("text", "string", "")], ret: "" },
    Func { ns: "log", name: "warn", doc: "Writes a warning to vana-client.log.", params: &[("text", "string", "")], ret: "" },
    Func { ns: "log", name: "error", doc: "Writes an error to vana-client.log.", params: &[("text", "string", "")], ret: "" },
    Func { ns: "log", name: "debug", doc: "Writes a debug line to vana-client.log (shown with debug logging on).", params: &[("text", "string", "")], ret: "" },
    Func { ns: "events", name: "on", doc: "Calls `handler` on every `event` (see the events list), as a coroutine that may sleep. Higher priority runs first.", params: &[("event", "vana.Event|string", "event name"), ("handler", "function", "its arguments are the event's"), ("options?", "{priority: integer?}", "")], ret: "integer" },
    Func { ns: "events", name: "off", doc: "Removes a handler added by events.on.", params: &[("id", "integer", "what events.on returned")], ret: "" },
    Func { ns: "commands", name: "register", doc: "Adds a command: typed as //name or /name, it calls `handler` with the words after it.", params: &[("name", "string", "without the slash"), ("handler", "fun(...: string)", "")], ret: "" },
    Func { ns: "commands", name: "run", doc: "Runs a line as if the player had typed it (game commands, our commands, other mods' commands).", params: &[("line", "string", "e.g. '/ma \"Cure\" <t>'")], ret: "" },
    Func { ns: "chat", name: "add", doc: "Adds a line to the chat log. With a mode it takes that mode's colour, filters and indent, like a game line.", params: &[("text", "string", ""), ("mode?", "integer", "the game's chat mode (0..255)")], ret: "" },
    Func { ns: "timer", name: "after", doc: "Calls `handler` once after `seconds`.", params: &[("seconds", "number", ""), ("handler", "function", "")], ret: "integer" },
    Func { ns: "timer", name: "every", doc: "Calls `handler` every `seconds` until cancelled.", params: &[("seconds", "number", "at least 0.01"), ("handler", "function", "")], ret: "integer" },
    Func { ns: "timer", name: "cancel", doc: "Stops a timer.", params: &[("id", "integer", "what timer.after / timer.every returned")], ret: "" },
    Func { ns: "game", name: "player", doc: "Our character: name, id, index, zone, pos {x, y, z}, heading, status, hp / mp / tp and their percents, maxima, jobs and levels, target id.", params: &[], ret: "vana.Player" },
    Func { ns: "game", name: "zone", doc: "The zone id, or nil while zoning.", params: &[], ret: "integer?" },
    Func { ns: "storage", name: "save", doc: "Writes storage.account and storage.character now (they are also written at logout and unload).", params: &[], ret: "" },
    Func { ns: "settings", name: "get", doc: "A setting declared in mod.toml: the user's value, else its default.", params: &[("key", "string", "")], ret: "any" },
];

pub const EVENTS: &[Event] = &[
    Event { name: "load", doc: "The mod's entry file has run.", args: &[] },
    Event { name: "unload", doc: "The mod is about to be unloaded (logout, /lua unload or reload).", args: &[] },
    Event { name: "login", doc: "The character is in the world, once per session.", args: &[("name", "string")] },
    Event { name: "logout", doc: "The character is leaving the world.", args: &[] },
    Event { name: "zone_change", doc: "A new zone has loaded.", args: &[("zone", "integer"), ("previous", "integer?")] },
    Event { name: "frame", doc: "Every frame (prefer timers and events: this one costs time every frame).", args: &[("dt", "number")] },
];

/// LuaLS stubs (`---@meta`) for the whole API.
pub fn stubs() -> String {
    let mut s = String::from("---@meta\n-- vana-client mod API (generated by `vana-client lua-stubs`; docs/lua-mods.md).\n\n");
    s += "---@alias vana.Event\n";
    for e in EVENTS {
        s += &format!("---| \"{}\" # {}\n", e.name, e.doc);
    }
    s += "\n---@class vana.Player\n---@field name string\n---@field id integer\n---@field index integer\n---@field zone integer?\n---@field pos {x: number, y: number, z: number}\n---@field heading number\n---@field status integer\n---@field hp integer?\n---@field mp integer?\n---@field tp integer?\n---@field hpp integer?\n---@field mpp integer?\n---@field hp_max integer?\n---@field mp_max integer?\n---@field main_job integer?\n---@field main_level integer?\n---@field sub_job integer?\n---@field sub_level integer?\n---@field target integer?\n\n";
    s += "---@class vana.Mod\n---@field id string\n---@field name string\n---@field version string\n---@field api integer\n---@field path string\n\n---@type vana.Mod\nmod = {}\n\n";
    s += "storage = {\n    ---@type table plain data kept for every character of the account\n    account = {},\n    ---@type table plain data kept for this character\n    character = {},\n}\n\n";
    let mut seen = Vec::new();
    for f in FUNCS {
        if !f.ns.is_empty() && !seen.contains(&f.ns) {
            seen.push(f.ns);
            if f.ns != "storage" {
                s += &format!("{} = {{}}\n", f.ns);
            }
        }
    }
    s += "\n";
    for f in FUNCS {
        for line in f.doc.lines() {
            s += &format!("--- {line}\n");
        }
        for (name, ty, doc) in f.params {
            s += &format!("---@param {name} {ty} {doc}\n");
        }
        if !f.ret.is_empty() {
            s += &format!("---@return {}\n", f.ret);
        }
        let args: Vec<&str> = f.params.iter().map(|p| p.0.trim_end_matches('?')).collect();
        let full = if f.ns.is_empty() { f.name.to_string() } else { format!("{}.{}", f.ns, f.name) };
        s += &format!("function {full}({}) end\n\n", args.join(", "));
    }
    s += "-- Events (events.on):\n";
    for e in EVENTS {
        let args: Vec<String> = e.args.iter().map(|(n, t)| format!("{n}: {t}")).collect();
        s += &format!("--   {}({}): {}\n", e.name, args.join(", "), e.doc);
    }
    s
}
