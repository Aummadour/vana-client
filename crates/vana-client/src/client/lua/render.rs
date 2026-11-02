//! Render overrides for mods (docs/lua-mods.md 4.11): how an entity draws (hidden, name, name colour, alpha, tint,
//! size) and the camera's distance. Looks last while the mod is loaded; the newest mod's wins per entity.

use super::api::{state, world};
use mlua::{Lua, Table, Value};

/// One entity's drawn look as a mod set it.
#[derive(Clone, Default)]
pub(in crate::client) struct Look {
    pub hidden: bool,
    pub name: Option<String>,
    /// RGBA, 0..255.
    pub name_colour: Option<[u8; 4]>,
    /// 0..1, at most the model's own.
    pub alpha: Option<f32>,
    /// RGB multipliers.
    pub tint: Option<[f32; 3]>,
    pub scale: Option<f32>,
}

impl Look {
    /// The plate's colour as the game's (ARGB, 0x80 = 1.0).
    pub fn plate_tint(&self) -> Option<u32> {
        let h = |v: u8| u8::try_from(u16::from(v).div_ceil(2)).unwrap_or(0x80);
        self.name_colour.map(|c| u32::from_be_bytes([h(c[3]), h(c[0]), h(c[1]), h(c[2])]))
    }
}

fn info(lua: &Lua) -> std::rc::Rc<std::cell::RefCell<super::state::FrameInfo>> {
    lua.app_data_ref::<std::rc::Rc<std::cell::RefCell<super::state::FrameInfo>>>().map(|f| f.clone()).unwrap_or_default()
}

fn colour(v: Value) -> mlua::Result<Option<[u8; 4]>> {
    Ok(match v {
        Value::Table(t) => {
            let get = |k: &str, i: i64, d: u8| -> mlua::Result<u8> { Ok(t.get::<Option<u8>>(k)?.or(t.get::<Option<u8>>(i)?).unwrap_or(d)) };
            Some([get("r", 1, 255)?, get("g", 2, 255)?, get("b", 3, 255)?, get("a", 4, 255)?])
        }
        _ => None,
    })
}

pub(super) fn install(lua: &Lua, g: &Table, mod_id: &str) -> mlua::Result<()> {
    let t = lua.create_table()?;
    let me = mod_id.to_string();
    t.set(
        "entity",
        lua.create_function(move |lua, (id, spec): (u32, Option<Table>)| {
            let info = info(lua);
            let mut f = info.borrow_mut();
            let Some(s) = spec else {
                if f.looks.get(&id).is_some_and(|l| l.0 == me) {
                    f.looks.remove(&id);
                }
                return Ok(());
            };
            let tint = match s.get::<Value>("tint")? {
                Value::Table(c) => Some([c.get::<Option<f32>>(1)?.unwrap_or(1.0), c.get::<Option<f32>>(2)?.unwrap_or(1.0), c.get::<Option<f32>>(3)?.unwrap_or(1.0)]),
                _ => None,
            };
            let look = Look {
                hidden: s.get::<Option<bool>>("hidden")?.unwrap_or(false),
                name: s.get::<Option<String>>("name")?,
                name_colour: colour(s.get("name_colour")?)?,
                alpha: s.get::<Option<f32>>("alpha")?.map(|a| a.clamp(0.0, 1.0)),
                tint,
                scale: s.get::<Option<f32>>("scale")?.map(|k| k.clamp(0.05, 20.0)),
            };
            f.looks.insert(id, (me.clone(), look));
            Ok(())
        })?,
    )?;
    let me = mod_id.to_string();
    t.set(
        "clear",
        lua.create_function(move |lua, ()| {
            info(lua).borrow_mut().looks.retain(|_, l| l.0 != me);
            Ok(())
        })?,
    )?;
    t.set("camera_distance", lua.create_function(|lua, ()| Ok(world(lua)?.camst.dist))?)?;
    t.set(
        "set_camera_distance",
        lua.create_function(|lua, k: f32| {
            state(lua).borrow_mut().ops.push(super::Op::CameraDistance(k));
            Ok(())
        })?,
    )?;
    g.set("render", t)?;
    Ok(())
}
