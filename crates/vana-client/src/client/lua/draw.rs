//! World-space drawing for mods (docs/lua-mods.md 4.10 "World space"): lines, rings and boxes in world coordinates (as
//! entity positions are), behind whatever stands in front of them, and world-to-screen for labels.

use super::World;
use crate::render::LineVertex;
use glam::Vec3;
use mlua::{Lua, Table, Value};

#[derive(Clone)]
pub(super) enum Kind {
    Line(Vec3, Vec3),
    /// Centre, radius, segments, filled.
    Ring(Vec3, f32, u32, bool),
    /// Centre and size (an axis-aligned wire box).
    Box(Vec3, Vec3),
}

#[derive(Clone)]
pub(super) struct Shape {
    pub id: String,
    pub kind: Kind,
    /// RGBA 0..255.
    pub colour: [u8; 4],
}

fn vec3(v: Value) -> mlua::Result<Vec3> {
    match v {
        Value::Table(t) => Ok(Vec3::new(t.get::<Option<f32>>("x")?.or(t.get(1)?).unwrap_or(0.0), t.get::<Option<f32>>("y")?.or(t.get(2)?).unwrap_or(0.0), t.get::<Option<f32>>("z")?.or(t.get(3)?).unwrap_or(0.0))),
        _ => Err(mlua::Error::runtime("a point is {x, y, z}")),
    }
}

fn colour(t: &Table) -> mlua::Result<[u8; 4]> {
    Ok(match t.get::<Value>("colour")? {
        Value::Table(c) => {
            let get = |k: &str, i: i64, d: u8| -> mlua::Result<u8> { Ok(c.get::<Option<u8>>(k)?.or(c.get::<Option<u8>>(i)?).unwrap_or(d)) };
            [get("r", 1, 255)?, get("g", 2, 255)?, get("b", 3, 255)?, get("a", 4, 255)?]
        }
        _ => [255, 255, 255, 255],
    })
}

fn put(lua: &Lua, id: String, kind: Kind, colour: [u8; 4]) -> mlua::Result<()> {
    let st = super::api::state(lua);
    let mut s = st.borrow_mut();
    let shape = Shape { id: id.clone(), kind, colour };
    match s.shapes.iter_mut().find(|x| x.id == id) {
        Some(old) => *old = shape,
        None => s.shapes.push(shape),
    }
    Ok(())
}

pub(super) fn install(lua: &Lua, g: &Table) -> mlua::Result<()> {
    let t = lua.create_table()?;
    t.set("line", lua.create_function(|lua, (id, s): (String, Table)| put(lua, id, Kind::Line(vec3(s.get("from")?)?, vec3(s.get("to")?)?), colour(&s)?))?)?;
    t.set(
        "ring",
        lua.create_function(|lua, (id, s): (String, Table)| {
            let segments = s.get::<Option<u32>>("segments")?.unwrap_or(48).clamp(3, 360);
            let kind = Kind::Ring(vec3(s.get("at")?)?, s.get::<Option<f32>>("radius")?.unwrap_or(1.0).max(0.0), segments, s.get::<Option<bool>>("filled")?.unwrap_or(false));
            put(lua, id, kind, colour(&s)?)
        })?,
    )?;
    t.set("box", lua.create_function(|lua, (id, s): (String, Table)| put(lua, id, Kind::Box(vec3(s.get("at")?)?, vec3(s.get("size")?)?), colour(&s)?))?)?;
    t.set(
        "remove",
        lua.create_function(|lua, id: String| {
            super::api::state(lua).borrow_mut().shapes.retain(|x| x.id != id);
            Ok(())
        })?,
    )?;
    t.set(
        "clear",
        lua.create_function(|lua, ()| {
            super::api::state(lua).borrow_mut().shapes.clear();
            Ok(())
        })?,
    )?;
    t.set(
        "to_screen",
        lua.create_function(|lua, p: Value| {
            let p = vec3(p)?;
            let info = lua.app_data_ref::<std::rc::Rc<std::cell::RefCell<super::state::FrameInfo>>>().map(|f| f.clone()).unwrap_or_default();
            let f = info.borrow();
            let clip = f.view_proj * p.extend(1.0);
            if clip.w <= 1e-4 {
                return Ok(None);
            }
            let ndc = clip.truncate() / clip.w;
            let (x, y) = ((ndc.x + 1.0) / 2.0 * f.window[0], (1.0 - ndc.y) / 2.0 * f.window[1]);
            let out = lua.create_table()?;
            out.set("x", x.round() as i64)?;
            out.set("y", y.round() as i64)?;
            out.set("on_screen", ndc.x.abs() <= 1.0 && ndc.y.abs() <= 1.0 && (0.0..=1.0).contains(&ndc.z))?;
            Ok(Some(out))
        })?,
    )?;
    g.set("draw", t)?;
    Ok(())
}

/// Every running mod's shapes as vertices: [line pairs, triangles].
pub(super) fn vertices(w: &World) -> [Vec<LineVertex>; 2] {
    let mut out: [Vec<LineVertex>; 2] = [Vec::new(), Vec::new()];
    for vm in w.lua.vms.iter().filter(|v| v.suspended.is_none()) {
        for s in vm.st.borrow().shapes.iter() {
            let color = s.colour.map(|c| c as f32 / 255.0);
            let v = |p: Vec3| LineVertex { pos: p.to_array(), color };
            match s.kind {
                Kind::Line(a, b) => out[0].extend([v(a), v(b)]),
                Kind::Ring(c, r, n, filled) => {
                    let at = |k: u32| {
                        let a = k as f32 / n as f32 * std::f32::consts::TAU;
                        c + Vec3::new(a.cos() * r, 0.0, a.sin() * r)
                    };
                    for k in 0..n {
                        if filled {
                            out[1].extend([v(c), v(at(k)), v(at(k + 1))]);
                        } else {
                            out[0].extend([v(at(k)), v(at(k + 1))]);
                        }
                    }
                }
                Kind::Box(c, size) => {
                    let h = size * 0.5;
                    let corner = |i: u32| c + Vec3::new(if i & 1 != 0 { h.x } else { -h.x }, if i & 2 != 0 { h.y } else { -h.y }, if i & 4 != 0 { h.z } else { -h.z });
                    for (a, b) in [(0, 1), (2, 3), (4, 5), (6, 7), (0, 2), (1, 3), (4, 6), (5, 7), (0, 4), (1, 5), (2, 6), (3, 7)] {
                        out[0].extend([v(corner(a)), v(corner(b))]);
                    }
                }
            }
        }
    }
    out
}
