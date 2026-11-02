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
    Func { ns: "filters", name: "chat", doc: "Runs `filter` on every chat line before it is kept: return nil to keep it, false to drop it, or new text (and optionally a new mode) to replace it. Runs inline: it cannot sleep or read game state.", params: &[("filter", "fun(text: string, mode: integer?, battle: boolean): (string|false|nil, integer?)", "")], ret: "integer" },
    Func { ns: "filters", name: "typed", doc: "Runs `filter` on every line typed or run (macros, mods) before the command parser: nil keeps it, false drops it, a string replaces it. Runs inline.", params: &[("filter", "fun(line: string): string|false|nil", "")], ret: "integer" },
    Func { ns: "filters", name: "off", doc: "Removes a filter.", params: &[("id", "integer", "what filters.chat / filters.typed returned")], ret: "" },
    Func { ns: "chat", name: "add", doc: "Adds a line to the chat log. With a mode it takes that mode's colour, filters and indent, like a game line.", params: &[("text", "string", ""), ("mode?", "integer", "the game's chat mode (0..255)")], ret: "" },
    Func { ns: "timer", name: "after", doc: "Calls `handler` once after `seconds`.", params: &[("seconds", "number", ""), ("handler", "function", "")], ret: "integer" },
    Func { ns: "timer", name: "every", doc: "Calls `handler` every `seconds` until cancelled.", params: &[("seconds", "number", "at least 0.01"), ("handler", "function", "")], ret: "integer" },
    Func { ns: "timer", name: "cancel", doc: "Stops a timer.", params: &[("id", "integer", "what timer.after / timer.every returned")], ret: "" },
    Func { ns: "game", name: "player", doc: "Our character: name, id, index, zone, pos {x, y, z}, heading, status, hp / mp / tp and their percents, maxima, jobs and levels, target id.", params: &[], ret: "vana.Player" },
    Func { ns: "game", name: "zone", doc: "The zone id, or nil while zoning.", params: &[], ret: "integer?" },
    Func { ns: "game", name: "entity", doc: "An entity in range by server id or index, or nil.", params: &[("key", "integer", "server id or index")], ret: "vana.Entity?" },
    Func { ns: "game", name: "entities", doc: "Every entity in range, by index.", params: &[("filter?", "{distance: number?, kinds: ('player'|'npc'|'mob')[]?}", "")], ret: "vana.Entity[]" },
    Func { ns: "game", name: "target", doc: "The current target, or nil.", params: &[], ret: "vana.Entity?" },
    Func { ns: "game", name: "party", doc: "The party and alliance members (slot 0..5 our party, 6..17 the others).", params: &[], ret: "vana.Member[]" },
    Func { ns: "game", name: "items", doc: "A bag's items in the list's order (0 inventory, 1 safe, 2 storage, 3 temporary, 4 locker, 5 satchel, 6 sack, 7 case, 8.. wardrobes).", params: &[("bag", "integer", "")], ret: "vana.Item[]" },
    Func { ns: "game", name: "bag_size", doc: "How many slots a bag has (nil before the server said).", params: &[("bag", "integer", "")], ret: "integer?" },
    Func { ns: "game", name: "equipment", doc: "What is equipped, by equipment slot (0 main .. 15 back): {bag, slot, id}.", params: &[], ret: "table<integer, {bag: integer, slot: integer, id: integer}>" },
    Func { ns: "game", name: "buffs", doc: "Our status effects: id, and seconds remaining when timed.", params: &[], ret: "{id: integer, remaining: number?}[]" },
    Func { ns: "game", name: "recasts", doc: "Seconds left on recasts: spells by spell id, abilities by recast id (only those still counting).", params: &[], ret: "{spells: table<integer, number>, abilities: table<integer, number>}" },
    Func { ns: "game", name: "time", doc: "The Vana'diel clock: earth (seconds since the Vana'diel epoch), year, month, day, hour, minute, weekday (0 Firesday), moon_phase, moon (percent).", params: &[], ret: "vana.Time" },
    Func { ns: "game", name: "weather", doc: "The weather id.", params: &[], ret: "integer" },
    Func { ns: "game", name: "key_items", doc: "The key item ids we hold.", params: &[], ret: "integer[]" },
    Func { ns: "res", name: "item", doc: "An item's record: name, description, kind, flags, stack, type, targets, level, slots, races, jobs (bits), superior_level, damage, delay, skill, max_charges, cast_time, use_delay, reuse_delay.", params: &[("id", "integer", "")], ret: "table?" },
    Func { ns: "res", name: "spell", doc: "A spell's record: name, description, kind (1 white .. 8 trust), element, targets (bits), skill, mp, cast_time and recast (seconds), range, levels (by job).", params: &[("id", "integer", "")], ret: "table?" },
    Func { ns: "res", name: "ability", doc: "An ability's record by its name-table id (weapon skills 0..0x1ff, job abilities 0x200.., pet commands 0x400..): name, description, kind, cost, recast_id, targets, tp, level.", params: &[("id", "integer", "")], ret: "table?" },
    Func { ns: "res", name: "status", doc: "A status effect's name.", params: &[("id", "integer", "")], ret: "string?" },
    Func { ns: "res", name: "zone", doc: "A zone's name.", params: &[("id", "integer", "")], ret: "string?" },
    Func { ns: "res", name: "title", doc: "A title's name.", params: &[("id", "integer", "")], ret: "string?" },
    Func { ns: "res", name: "key_item", doc: "A key item's name and description.", params: &[("id", "integer", "")], ret: "{id: integer, name: string, description: string?}?" },
    Func { ns: "res", name: "job", doc: "A job's abbreviation (1 WAR .. 22 RUN).", params: &[("id", "integer", "")], ret: "string?" },
    Func { ns: "storage", name: "save", doc: "Writes storage.account and storage.character now (they are also written at logout and unload).", params: &[], ret: "" },
    Func { ns: "settings", name: "get", doc: "A setting declared in mod.toml: the user's value, else its default.", params: &[("key", "string", "")], ret: "any" },
];

pub const EVENTS: &[Event] = &[
    Event { name: "load", doc: "The mod's entry file has run.", args: &[] },
    Event { name: "unload", doc: "The mod is about to be unloaded (logout, /lua unload or reload).", args: &[] },
    Event { name: "login", doc: "The character is in the world, once per session.", args: &[("name", "string")] },
    Event { name: "logout", doc: "The character is leaving the world.", args: &[] },
    Event { name: "zone_change", doc: "A new zone has loaded.", args: &[("zone", "integer"), ("previous", "integer?")] },
    Event { name: "settings_changed", doc: "The user changed one of the mod's settings (its Extras page, or /lua set).", args: &[("key", "string"), ("value", "any")] },
    Event { name: "frame", doc: "Every frame (prefer timers and events: this one costs time every frame).", args: &[("dt", "number")] },
    Event { name: "hp_change", doc: "Our HP changed.", args: &[("hp", "integer"), ("previous", "integer")] },
    Event { name: "mp_change", doc: "Our MP changed.", args: &[("mp", "integer"), ("previous", "integer")] },
    Event { name: "tp_change", doc: "Our TP changed.", args: &[("tp", "integer"), ("previous", "integer")] },
    Event { name: "hpp_change", doc: "Our HP percent changed.", args: &[("hpp", "integer"), ("previous", "integer")] },
    Event { name: "mpp_change", doc: "Our MP percent changed.", args: &[("mpp", "integer"), ("previous", "integer")] },
    Event { name: "status_change", doc: "Our status changed (0 idle, 1 engaged, 2 / 3 dead, 33 resting...).", args: &[("status", "integer"), ("previous", "integer")] },
    Event { name: "job_change", doc: "Our main or sub job changed.", args: &[("main", "integer"), ("main_level", "integer"), ("sub", "integer"), ("sub_level", "integer")] },
    Event { name: "level_up", doc: "Our main job's level went up.", args: &[("level", "integer")] },
    Event { name: "level_down", doc: "Our main job's level went down.", args: &[("level", "integer")] },
    Event { name: "buff_gain", doc: "We gained a status effect.", args: &[("id", "integer")] },
    Event { name: "buff_lose", doc: "We lost a status effect.", args: &[("id", "integer")] },
    Event { name: "target_change", doc: "Our target changed (nil: none).", args: &[("id", "integer?"), ("previous", "integer?")] },
    Event { name: "time_change", doc: "A Vana'diel minute passed (minutes since midnight).", args: &[("minute", "integer"), ("previous", "integer")] },
    Event { name: "day_change", doc: "A new Vana'diel day (0 Firesday).", args: &[("weekday", "integer"), ("previous", "integer")] },
    Event { name: "moon_change", doc: "The moon's phase changed.", args: &[("phase", "integer"), ("previous", "integer")] },
    Event { name: "weather_change", doc: "The weather changed.", args: &[("weather", "integer"), ("previous", "integer")] },
    Event { name: "party_change", doc: "The party or alliance changed (members joined, left or moved).", args: &[] },
    Event { name: "item_add", doc: "An item appeared in a bag slot.", args: &[("bag", "integer"), ("slot", "integer"), ("id", "integer"), ("count", "integer")] },
    Event { name: "item_remove", doc: "An item left a bag slot.", args: &[("bag", "integer"), ("slot", "integer"), ("id", "integer"), ("count", "integer")] },
    Event { name: "chat_message", doc: "A chat line from a player (or a GM): its text, sender, kind (0 say, 1 shout, 3 tell, 4 party, 5 linkshell, 0x1a yell, 0x1b linkshell 2, 0x21 unity) and whether a GM sent it.", args: &[("text", "string"), ("sender", "string"), ("kind", "integer"), ("gm", "boolean")] },
    Event { name: "action", doc: "A decoded action packet (s2c 0x028): actor, category, param, info and targets with their results.", args: &[("action", "vana.Action")] },
    Event { name: "battle_message", doc: "A battle message (s2c 0x029 / 0x02D): actor, target, message id and its two parameters.", args: &[("actor", "integer"), ("target", "integer"), ("message", "integer"), ("param1", "integer"), ("param2", "integer")] },
    Event { name: "item_change", doc: "A bag slot's count changed.", args: &[("bag", "integer"), ("slot", "integer"), ("id", "integer"), ("count", "integer"), ("previous", "integer")] },
];

/// LuaLS stubs (`---@meta`) for the whole API.
pub fn stubs() -> String {
    let mut s = String::from("---@meta\n-- vana-client mod API (generated by `vana-client lua-stubs`; docs/lua-mods.md).\n\n");
    s += "---@alias vana.Event\n";
    for e in EVENTS {
        s += &format!("---| \"{}\" # {}\n", e.name, e.doc);
    }
    s += "\n---@class vana.Player\n---@field name string\n---@field id integer\n---@field index integer\n---@field zone integer?\n---@field pos {x: number, y: number, z: number}\n---@field heading number\n---@field status integer\n---@field hp integer?\n---@field mp integer?\n---@field tp integer?\n---@field hpp integer?\n---@field mpp integer?\n---@field hp_max integer?\n---@field mp_max integer?\n---@field main_job integer?\n---@field main_level integer?\n---@field sub_job integer?\n---@field sub_level integer?\n---@field target integer?\n\n";
    s += "---@class vana.Entity\n---@field id integer\n---@field index integer\n---@field name string\n---@field kind 'player'|'npc'|'mob'\n---@field pos {x: number, y: number, z: number}\n---@field heading number\n---@field distance number\n---@field hpp integer\n---@field status integer\n---@field speed number\n---@field claim integer\n---@field targetable boolean\n---@field hidden boolean\n---@field me boolean? true for our own character\n\n";
    s += "---@class vana.Member\n---@field slot integer\n---@field party integer\n---@field id integer\n---@field index integer\n---@field name string\n---@field hp integer\n---@field mp integer\n---@field tp integer\n---@field hpp integer\n---@field mpp integer\n---@field zone integer\n---@field in_zone boolean\n---@field main_job integer\n---@field main_level integer\n---@field sub_job integer\n---@field sub_level integer\n---@field leader boolean\n---@field alliance_leader boolean\n---@field buffs integer[]\n\n";
    s += "---@class vana.Item\n---@field bag integer\n---@field slot integer\n---@field id integer\n---@field count integer\n---@field extdata string? 24 raw bytes\n---@field locked integer\n---@field price integer?\n\n";
    s += "---@class vana.Action\n---@field actor integer\n---@field category integer\n---@field param integer\n---@field info integer\n---@field targets {id: integer, results: {miss: integer, kind: integer, animation: integer, info: integer, scale: integer, value: integer, message: integer, bit: integer, proc: table?, react: table?}[]}[]\n\n";
    s += "---@class vana.Time\n---@field earth integer\n---@field year integer\n---@field month integer\n---@field day integer\n---@field hour integer\n---@field minute integer\n---@field weekday integer\n---@field moon_phase integer\n---@field moon integer\n\n";
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
