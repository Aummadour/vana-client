//! The Lua mods' windows in the modern UI (docs/lua-mods.md 4.10): each open window a panel of its own, its place kept
//! as `mod:<mod>:<window>`, its elements the kit's controls. Keys are the window model's (client/lua/window.rs); the
//! mouse goes back to it as clicks.

use super::super::World;
use super::super::lua::window::Elem;
use super::{Panels, Title, kit};
use egui::{Align2, FontId, RichText, pos2};

/// What the pointer did on a window, for the model.
pub enum Click {
    Button(usize, usize),
    Slide(usize, usize),
    Row(usize, usize),
}

pub fn draw(p: &mut Panels, w: &mut World) {
    let m = p.m;
    for (mod_id, win) in w.lua.open_windows() {
        let id = format!("mod:{mod_id}:{}", win.id);
        let width = if win.width > 0.0 { win.width } else { 360.0 };
        let mut clicks: Vec<Click> = Vec::new();
        let title = (!win.title.is_empty()).then_some(Title { icon: None, text: &win.title, note: None });
        let frame = if win.framed { "window_normal" } else { "none" };
        p.framed(&id, title, width, frame, |ui| {
            for (i, e) in win.elems.iter().enumerate() {
                let here = win.focus && i == win.at;
                match e {
                    Elem::Text { text, dim, .. } => {
                        let c = if *dim { m.theme.color("muted") } else { m.theme.color("text") };
                        ui.label(RichText::new(text).size(m.theme.small).color(c));
                        ui.add_space(4.0);
                    }
                    Elem::Buttons { label, items, marked, .. } => {
                        let labels: Vec<&str> = items.iter().map(String::as_str).collect();
                        let lit = marked.map_or(0, |k| 1u64 << k.min(63));
                        let cursor = (here && !win.edit).then_some(win.col);
                        let (hit, _) = kit::setting(ui, m, label, here, false, here, |ui| kit::buttons(ui, m, &labels, cursor, lit, 0));
                        if let Some(c) = hit {
                            clicks.push(Click::Button(i, c));
                        }
                    }
                    Elem::Slider { label, n, at, shown, .. } => {
                        let (hit, _) = kit::slider_setting(ui, m, label, here, here, *n, *at, shown);
                        if let Some(s) = hit {
                            clicks.push(Click::Slide(i, s));
                        }
                    }
                    Elem::List { id, rows, at, height, .. } => {
                        let h = 26.0;
                        let out = kit::list(ui, m, ("mod list", &win.id, id), rows.len(), Some(*at), here, here, h, (h + 2.0) * *height as f32, |_| false, |ui, row| {
                            let text = &rows[row.index];
                            ui.painter().text(pos2(row.rect.min.x + 10.0, row.rect.center().y), Align2::LEFT_CENTER, text, FontId::proportional(m.theme.small), kit::ink(m, row));
                        });
                        if let Some(k) = out.clicked {
                            clicks.push(Click::Row(i, k));
                        }
                        ui.add_space(6.0);
                    }
                    Elem::Gauge { label, value, text, kind, .. } => {
                        let (r, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 33.0), egui::Sense::hover());
                        let bar = egui::Rect::from_min_max(pos2(r.min.x, r.max.y - super::hud::BAR), r.max);
                        super::hud::labelled_bar(ui, m, kind, bar, value.clamp(0.0, 1.0), (label, m.theme.color("text")), (text, m.theme.color("text")));
                    }
                    Elem::Separator => {
                        ui.separator();
                    }
                }
            }
            if !win.help.is_empty() {
                ui.label(RichText::new(&win.help).size(m.theme.small).color(m.theme.color("muted")));
            }
        });
        for c in clicks {
            super::super::lua::window_click(w, &mod_id, &win.id, c);
        }
    }
}
