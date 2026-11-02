//! The rest of the game façade (docs/lua-mods.md 4.6): skills, jobs, merits and job points, stats, gil, what we have
//! learned, job setups, session flags, the camera, targets by tag and the zone's NPC list.

use super::api::world;
use super::World;
use mlua::{Lua, Table};

/// What the mod phase reads from the UI for the mods (shared as app data): the map grid as <pos> shows it.
#[derive(Default)]
pub(super) struct FrameInfo {
    pub grid: String,
    grid_at: Option<std::time::Instant>,
}

impl FrameInfo {
    /// The grid square, every half second (both UIs' location panels do the same).
    pub fn update(&mut self, w: &World, gu: &super::super::game_ui::GameUi) {
        if self.grid_at.is_none_or(|t| t.elapsed().as_secs_f32() >= 0.5) {
            self.grid_at = Some(std::time::Instant::now());
            self.grid = super::super::map::grid_text(w, gu.map_regions());
        }
    }
}

fn vec3(lua: &Lua, v: glam::Vec3) -> mlua::Result<Table> {
    let t = lua.create_table()?;
    t.set("x", v.x)?;
    t.set("y", v.y)?;
    t.set("z", v.z)?;
    Ok(t)
}

/// A target tag ("t", "<st>", "bt", "pet", "me", "ht", "scan", "p0".."p5", "a10".."a25"), a player's name or a server
/// id: (id, index).
pub(super) fn by_tag(w: &World, tag: &str) -> Option<(u32, u16)> {
    let inner = tag.trim().trim_start_matches('<').trim_end_matches('>').to_ascii_lowercase();
    let slot = match inner.as_bytes() {
        [b'p', d @ b'0'..=b'5'] => Some((d - b'0') as usize),
        [b'a', p @ b'1'..=b'2', d @ b'0'..=b'5'] => Some((p - b'0') as usize * 6 + (d - b'0') as usize),
        _ => None,
    };
    if let Some(k) = slot {
        let m = w.party.slots().slots.get(k).cloned().flatten()?;
        return Some((m.id, m.index));
    }
    let tagged = format!("<{inner}>");
    let probe = if ["t", "me", "bt", "scan", "pet", "ht"].contains(&inner.as_str()) { tagged.as_str() } else { tag.trim() };
    super::super::textcmd::resolve(w, Some(probe), false, true, super::super::textcmd::Mode::Internal)
}

pub(super) fn install(lua: &Lua, game: &Table) -> mlua::Result<()> {
    game.set(
        "entity_by_tag",
        lua.create_function(|lua, tag: String| {
            let w = world(lua)?;
            let Some((id, _)) = by_tag(w, &tag) else { return Ok(None) };
            if id == w.my_id {
                return super::api::self_entity(lua, w).map(Some);
            }
            w.entities.get(&id).map(|e| super::api::entity(lua, w, id, e)).transpose()
        })?,
    )?;
    game.set(
        "skills",
        lua.create_function(|lua, ()| {
            let w = world(lua)?;
            let out = lua.create_table()?;
            for (key, magic) in [("combat", false), ("magic", true)] {
                let t = lua.create_table()?;
                for (id, v) in super::super::status::skill_rows(w, magic) {
                    let s = lua.create_table()?;
                    s.set("level", v & 0x7fff)?;
                    s.set("capped", v & 0x8000 != 0)?;
                    t.set(id, s)?;
                }
                out.set(key, t)?;
            }
            let crafts = lua.create_table()?;
            for (id, v) in super::super::status::craft_rows(w) {
                let s = lua.create_table()?;
                s.set("rank", v & 0x1f)?;
                s.set("level", (v >> 5) & 0x3ff)?;
                s.set("capped", v & 0x8000 != 0)?;
                crafts.set(id, s)?;
            }
            out.set("crafts", crafts)?;
            Ok(out)
        })?,
    )?;
    game.set(
        "jobs",
        lua.create_function(|lua, ()| {
            let w = world(lua)?;
            let out = lua.create_table()?;
            let levels = lua.create_table()?;
            for (job, lv) in super::super::status::job_rows(w, false) {
                levels.set(job, lv)?;
            }
            let master = lua.create_table()?;
            for (job, lv) in super::super::status::job_rows(w, true) {
                master.set(job, lv)?;
            }
            out.set("levels", levels)?;
            out.set("master", master)?;
            Ok(out)
        })?,
    )?;
    game.set(
        "stats",
        lua.create_function(|lua, ()| {
            let w = world(lua)?;
            let Some(s) = w.stats.as_ref() else { return Ok(None) };
            let t = lua.create_table()?;
            t.set("base", s.base.to_vec())?;
            t.set("bonus", s.adj.to_vec())?;
            t.set("attack", s.atk)?;
            t.set("defense", s.def)?;
            t.set("resist", s.elem.to_vec())?;
            t.set("item_level", s.ilvl)?;
            t.set("superior_level", s.su)?;
            t.set("title", s.title)?;
            t.set("rank", s.rank)?;
            t.set("rank_points", s.rank_points)?;
            t.set("nation", s.nation)?;
            t.set("home_point", s.home)?;
            t.set("exp", s.exp.0)?;
            t.set("exp_next", s.exp.1)?;
            t.set("master_level", s.ml.0)?;
            t.set("exemplar", s.ml_exp.0)?;
            t.set("exemplar_next", s.ml_exp.1)?;
            if let Some((limit, bits, max)) = w.merits {
                t.set("limit_points", limit)?;
                t.set("merit_points", bits & 0x7f)?;
                t.set("merit_max", max)?;
            }
            Ok(Some(t))
        })?,
    )?;
    game.set(
        "merits",
        lua.create_function(|lua, ()| {
            let w = world(lua)?;
            let out = lua.create_table()?;
            for (&id, &(next, level)) in &w.merit_list {
                let m = lua.create_table()?;
                m.set("level", level)?;
                m.set("next_cost", next)?;
                out.set(id, m)?;
            }
            Ok(out)
        })?,
    )?;
    game.set(
        "job_points",
        lua.create_function(|lua, ()| {
            let w = world(lua)?;
            let out = lua.create_table()?;
            if let Some(jobs) = &w.jp_jobs {
                for (job, &[capacity, current, spent]) in jobs.iter().enumerate() {
                    let j = lua.create_table()?;
                    j.set("capacity", capacity)?;
                    j.set("points", current)?;
                    j.set("spent", spent)?;
                    out.set(job, j)?;
                }
            }
            Ok(out)
        })?,
    )?;
    game.set("gil", lua.create_function(|lua, ()| Ok(world(lua)?.inv.gil()))?)?;
    game.set(
        "spells",
        lua.create_function(|lua, ()| {
            let w = world(lua)?;
            Ok((0..(w.spells_known.len() * 8) as u16).filter(|&s| w.spells_known.get(s as usize / 8).is_some_and(|b| b & (1 << (s % 8)) != 0)).collect::<Vec<u16>>())
        })?,
    )?;
    game.set(
        "abilities",
        lua.create_function(|lua, ()| {
            let w = world(lua)?;
            let Some(acts) = w.actions.as_ref() else { return Ok(Vec::new()) };
            Ok(super::super::magic::ability_rows(w, 0).into_iter().filter_map(|i| acts.abilities.get(i as usize).map(|a| a.id)).collect::<Vec<u16>>())
        })?,
    )?;
    game.set(
        "job_data",
        lua.create_function(|lua, ()| {
            let w = world(lua)?;
            let out = lua.create_table()?;
            for (side, key) in [(0usize, "main"), (1, "sub")] {
                let b = &w.ext_job[side];
                if b[0] == 0 {
                    continue;
                }
                let t = lua.create_table()?;
                t.set("job", b[0])?;
                t.set("raw", lua.create_string(&b[..])?)?;
                match b[0] {
                    // BLU: 20 slots, spell id - 0x200.
                    0x10 => t.set("blue_spells", b[4..24].iter().filter(|&&s| s != 0).map(|&s| 0x200 + s as u16).collect::<Vec<u16>>())?,
                    // PUP: head and frame items 0x2000 + n, attachments 0x2100 + n.
                    0x12 => {
                        let item = |i: usize, v: u8| (v != 0).then(|| if i < 2 { 0x2000 + v as u16 } else { 0x2100 + v as u16 });
                        t.set("head", item(0, b[4]))?;
                        t.set("frame", item(1, b[5]))?;
                        t.set("attachments", (2..14).filter_map(|i| item(i, b[4 + i])).collect::<Vec<u16>>())?;
                    }
                    _ => {}
                }
                out.set(key, t)?;
            }
            Ok(out)
        })?,
    )?;
    game.set(
        "info",
        lua.create_function(|lua, ()| {
            let w = world(lua)?;
            let t = lua.create_table()?;
            t.set("zone", w.zone)?;
            t.set("server", w.world_name.as_str())?;
            t.set("mog_house", w.room_flag)?;
            t.set("chat_open", w.typing)?;
            t.set("menu_open", super::super::world_window_open(w))?;
            t.set("event", w.event.is_some())?;
            t.set("in_combat", w.my_motion.status == 1)?;
            t.set("locked", w.locked)?;
            t.set("autorun", w.autorun)?;
            t.set("follow", w.follow)?;
            t.set("pet_index", (w.my_pet != 0).then_some(w.my_pet))?;
            let grid = lua.app_data_ref::<std::rc::Rc<std::cell::RefCell<FrameInfo>>>().map(|f| f.borrow().grid.clone()).unwrap_or_default();
            t.set("map_grid", Some(grid).filter(|g| !g.is_empty() && !w.room_flag))?;
            t.set("modern_ui", w.modern)?;
            Ok(t)
        })?,
    )?;
    game.set(
        "camera",
        lua.create_function(|lua, ()| {
            let w = world(lua)?;
            let t = lua.create_table()?;
            t.set("eye", vec3(lua, w.cam.eye)?)?;
            t.set("look", vec3(lua, w.cam.look)?)?;
            t.set("distance", (w.cam.look - w.cam.eye).length())?;
            Ok(t)
        })?,
    )?;
    game.set(
        "npc_names",
        lua.create_function(|lua, ()| {
            let w = world(lua)?;
            let out = lua.create_table()?;
            for (&id, name) in &w.npc_names {
                out.set(id, name.as_str())?;
            }
            Ok(out)
        })?,
    )?;
    Ok(())
}
