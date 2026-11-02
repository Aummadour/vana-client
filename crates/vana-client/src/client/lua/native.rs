//! The game's own windows for mods (docs/lua-mods.md 4.10 "Native windows"): which are open, the interface and HUD
//! pieces hidden, the game config by id. Changing anything needs ui.native.

use super::api::{need, state, world};
use mlua::{Lua, Table};

/// HUD pieces a mod may hide, and the modern panels each is.
pub(super) const PIECES: &[(&str, &[&str])] = &[
    ("party", &["party", "alliance1", "alliance2"]),
    ("target", &["target"]),
    ("buffs", &["status"]),
    ("compass", &["where"]),
    ("minimap", &["minimap"]),
    ("cast", &["cast"]),
    ("plates", &[]),
];

fn info(lua: &Lua) -> std::rc::Rc<std::cell::RefCell<super::state::FrameInfo>> {
    lua.app_data_ref::<std::rc::Rc<std::cell::RefCell<super::state::FrameInfo>>>().map(|f| f.clone()).unwrap_or_default()
}

pub(super) fn install(lua: &Lua, ui: &Table, mod_id: &str) -> mlua::Result<()> {
    let t = lua.create_table()?;
    t.set("windows", lua.create_function(|lua, ()| Ok(super::super::open_windows(world(lua)?)))?)?;
    t.set("menu_open", lua.create_function(|lua, ()| Ok(super::super::world_window_open(world(lua)?)))?)?;
    t.set("interface_hidden", lua.create_function(|lua, ()| Ok(world(lua)?.ui_hidden))?)?;
    t.set(
        "interface",
        lua.create_function(|lua, show: bool| {
            need(lua, "ui.native")?;
            state(lua).borrow_mut().ops.push(super::Op::Interface(show));
            Ok(())
        })?,
    )?;
    let me = mod_id.to_string();
    t.set(
        "hide",
        lua.create_function(move |lua, (what, on): (String, Option<bool>)| {
            need(lua, "ui.native")?;
            if !PIECES.iter().any(|p| p.0 == what) {
                let names: Vec<&str> = PIECES.iter().map(|p| p.0).collect();
                return Err(mlua::Error::runtime(format!("no HUD piece {what:?} ({})", names.join(", "))));
            }
            let info = info(lua);
            let mut f = info.borrow_mut();
            let mods = f.hidden.entry(what).or_default();
            if on.unwrap_or(true) {
                mods.insert(me.clone());
            } else {
                mods.remove(&me);
            }
            Ok(())
        })?,
    )?;
    t.set(
        "hidden",
        lua.create_function(|lua, what: String| Ok(info(lua).borrow().hidden.get(&what).is_some_and(|m| !m.is_empty())))?,
    )?;
    t.set("config", lua.create_function(|lua, id: u16| Ok(world(lua)?.game.get(id)))?)?;
    t.set(
        "set_config",
        lua.create_function(|lua, (id, value): (u16, u32)| {
            need(lua, "ui.native")?;
            state(lua).borrow_mut().ops.push(super::Op::Config(id, value));
            Ok(())
        })?,
    )?;
    ui.set("native", t)?;
    Ok(())
}
