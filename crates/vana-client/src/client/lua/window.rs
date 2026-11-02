//! Mod windows (docs/lua-mods.md 4.10): a mod describes a window as elements; this module keeps its cursor and keys
//! (the same in both UIs) and draws it in the classic UI as the Config pages are drawn. The modern UI draws the same
//! model with the kit (modern/modwin.rs).

use super::super::game_ui::GameUi;
use crate::audio::Ui as Se;

#[derive(Clone, Debug, PartialEq)]
pub enum Elem {
    /// `id` (may be empty) lets ui.update find it.
    Text { id: String, text: String, dim: bool },
    /// A row of buttons under a heading; `marked` makes them options (the chosen one marked, Enter sets it).
    Buttons { id: String, label: String, items: Vec<String>, marked: Option<usize> },
    Slider { id: String, label: String, n: usize, at: usize, shown: String },
    /// Rows of text with a cursor; `height` rows show at once.
    List { id: String, rows: Vec<String>, at: usize, top: usize, height: usize },
    /// A bar filled `value` (0..1) with its label and text; `kind` is the art pack's gauge (hp, mp, tp, cast,
    /// experience, recast).
    Gauge { id: String, label: String, value: f32, text: String, kind: String },
    Separator,
}

impl Elem {
    pub fn stop(&self) -> bool {
        matches!(self, Elem::Buttons { .. } | Elem::Slider { .. } | Elem::List { .. })
    }

    pub fn id(&self) -> Option<&str> {
        match self {
            Elem::Buttons { id, .. } | Elem::Slider { id, .. } | Elem::List { id, .. } => Some(id),
            Elem::Text { id, .. } | Elem::Gauge { id, .. } if !id.is_empty() => Some(id),
            _ => None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Window {
    pub id: String,
    pub title: String,
    /// Points in the modern UI, pixels of the menu layout in the classic one.
    pub width: f32,
    pub help: String,
    pub elems: Vec<Elem>,
    pub open: bool,
    pub focus: bool,
    /// The cursor's element (an index into `elems`, a stop), its column, and a slider being edited.
    pub at: usize,
    pub col: usize,
    pub edit: bool,
    /// A HUD window never takes the keys (a click still acts); drawn without a frame when `framed` is off; at
    /// `place` (classic UI pixels) when given.
    pub hud: bool,
    pub framed: bool,
    pub place: Option<(f32, f32)>,
}

/// What happened in a window, for the mod's handler.
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    Press { id: String, index: usize },
    Set { id: String, index: usize },
    Select { id: String, index: usize },
    Close,
}

impl Event {
    pub fn json(&self) -> serde_json::Value {
        use serde_json::json;
        match self {
            Event::Press { id, index } => json!({"kind": "press", "id": id, "index": index + 1}),
            Event::Set { id, index } => json!({"kind": "set", "id": id, "index": index + 1}),
            Event::Select { id, index } => json!({"kind": "select", "id": id, "index": index + 1}),
            Event::Close => json!({"kind": "close"}),
        }
    }
}

/// The classic page's width when none is given (the Config pages').
pub const CLASSIC_W: f32 = 366.0;
const PITCH: f32 = 34.0;
const LINE: f32 = 18.0;
const TOP: f32 = 34.0;
const SLIDER_X: f32 = 88.0;

impl Window {
    pub fn new(id: &str) -> Self {
        Window { id: id.into(), title: String::new(), width: 0.0, help: String::new(), elems: Vec::new(), open: false, focus: false, at: 0, col: 0, edit: false, hud: false, framed: true, place: None }
    }

    fn stops(&self) -> Vec<usize> {
        self.elems.iter().enumerate().filter(|(_, e)| e.stop()).map(|(i, _)| i).collect()
    }

    /// The cursor on a stop (the first when it is not on one), its column on the marked option.
    pub fn settle(&mut self) {
        let stops = self.stops();
        if !stops.contains(&self.at) {
            self.at = stops.first().copied().unwrap_or(0);
            self.col = self.marked_col();
        }
        if let Some(Elem::Buttons { items, .. }) = self.elems.get(self.at) {
            self.col = self.col.min(items.len().saturating_sub(1));
        }
        if let Some(Elem::List { rows, at, top, height, .. }) = self.elems.get_mut(self.at) {
            *at = (*at).min(rows.len().saturating_sub(1));
            *top = (*top).min(*at).max((*at + 1).saturating_sub(*height));
        }
    }

    fn marked_col(&self) -> usize {
        match self.elems.get(self.at) {
            Some(Elem::Buttons { marked: Some(m), .. }) => *m,
            _ => 0,
        }
    }

    /// A key while the window has the focus: the events for the mod and the sounds.
    pub fn key(&mut self, key: egui::Key) -> (Vec<Event>, Vec<Se>) {
        use egui::Key as K;
        let (mut ev, mut se) = (Vec::new(), Vec::new());
        self.settle();
        if self.edit {
            if let Some(Elem::Slider { id, n, at, .. }) = self.elems.get_mut(self.at) {
                match key {
                    K::ArrowRight if *at + 1 < *n => {
                        *at += 1;
                        ev.push(Event::Set { id: id.clone(), index: *at });
                        se.push(Se::Cursor);
                    }
                    K::ArrowLeft if *at > 0 => {
                        *at -= 1;
                        ev.push(Event::Set { id: id.clone(), index: *at });
                        se.push(Se::Cursor);
                    }
                    K::Enter | K::Escape => {
                        self.edit = false;
                        se.push(if key == K::Enter { Se::Select } else { Se::Cancel });
                    }
                    _ => {}
                }
            } else {
                self.edit = false;
            }
            return (ev, se);
        }
        let stops = self.stops();
        let pos = stops.iter().position(|s| *s == self.at);
        match key {
            K::ArrowUp | K::ArrowDown => {
                let down = key == K::ArrowDown;
                // A list moves its own cursor until its end.
                if let Some(Elem::List { rows, at, top, height, .. }) = self.elems.get_mut(self.at) {
                    let inside = if down { *at + 1 < rows.len() } else { *at > 0 };
                    if inside {
                        *at = if down { *at + 1 } else { *at - 1 };
                        *top = (*top).min(*at).max((*at + 1).saturating_sub(*height));
                        se.push(Se::Cursor);
                        return (ev, se);
                    }
                }
                if let Some(p) = pos
                    && stops.len() > 1
                {
                    let n = stops.len();
                    self.at = stops[if down { (p + 1) % n } else { (p + n - 1) % n }];
                    self.col = self.marked_col();
                    if let Some(Elem::List { rows, at, top, height, .. }) = self.elems.get_mut(self.at) {
                        *at = if down { 0 } else { rows.len().saturating_sub(1) };
                        *top = (*top).min(*at).max((*at + 1).saturating_sub(*height));
                    }
                    se.push(Se::Cursor);
                }
            }
            K::ArrowLeft | K::ArrowRight => {
                if let Some(Elem::Buttons { items, .. }) = self.elems.get(self.at)
                    && items.len() > 1
                {
                    let n = items.len();
                    self.col = if key == K::ArrowRight { (self.col + 1) % n } else { (self.col + n - 1) % n };
                    se.push(Se::Cursor);
                }
            }
            K::Enter => match self.elems.get_mut(self.at) {
                Some(Elem::Buttons { id, marked, .. }) => {
                    match marked {
                        Some(m) if *m == self.col => {}
                        Some(m) => {
                            *m = self.col;
                            ev.push(Event::Set { id: id.clone(), index: self.col });
                        }
                        None => ev.push(Event::Press { id: id.clone(), index: self.col }),
                    }
                    se.push(Se::Select);
                }
                Some(Elem::Slider { .. }) => {
                    self.edit = true;
                    se.push(Se::Select);
                }
                Some(Elem::List { id, rows, at, .. }) if !rows.is_empty() => {
                    ev.push(Event::Select { id: id.clone(), index: *at });
                    se.push(Se::Select);
                }
                _ => {}
            },
            K::Escape => {
                self.open = false;
                self.focus = false;
                ev.push(Event::Close);
                se.push(Se::Cancel);
            }
            _ => {}
        }
        (ev, se)
    }

    /// The pointer on element `i`: the focus and cursor go there, then a button is pressed (as Enter), a slider set,
    /// a list row picked.
    pub fn click_button(&mut self, i: usize, col: usize) -> (Vec<Event>, Vec<Se>) {
        (self.at, self.col, self.edit) = (i, col, false);
        self.key(egui::Key::Enter)
    }

    pub fn slide(&mut self, i: usize, stop: usize) -> (Vec<Event>, Vec<Se>) {
        self.edit = false;
        self.at = i;
        if let Some(Elem::Slider { id, n, at, .. }) = self.elems.get_mut(i)
            && stop < *n
            && stop != *at
        {
            *at = stop;
            return (vec![Event::Set { id: id.clone(), index: stop }], vec![Se::Cursor]);
        }
        (Vec::new(), Vec::new())
    }

    pub fn pick(&mut self, i: usize, row: usize) -> (Vec<Event>, Vec<Se>) {
        self.edit = false;
        self.at = i;
        if let Some(Elem::List { at, .. }) = self.elems.get_mut(i) {
            *at = row;
        }
        self.key(egui::Key::Enter)
    }

    /// The classic page's controls at (x, y), as drawn: (element, column or list row, rect).
    fn classic_controls(&self, gu: &GameUi, x: f32, y: f32) -> Vec<(usize, usize, [f32; 4])> {
        let (w, _) = self.classic_size();
        let mut out = Vec::new();
        let mut ey = y + TOP;
        for (i, e) in self.elems.iter().enumerate() {
            match e {
                Elem::Buttons { items, .. } => {
                    let need = items.iter().map(|s| gu.label_width(s) + 12.0).fold(0.0, f32::max).ceil();
                    let bw = if need <= 64.0 && items.len() >= 3 { 64.0 } else if need <= 88.0 { 88.0 } else { need };
                    for c in 0..items.len() {
                        out.push((i, c, [x + 32.0 + (bw + 8.0) * c as f32, ey + 14.0, bw, 16.0]));
                    }
                }
                Elem::Slider { .. } => out.push((i, 0, [x + SLIDER_X, ey + 14.0, (w - SLIDER_X - 48.0).max(96.0), 16.0])),
                Elem::List { rows, top, height, .. } => {
                    for k in (*top..rows.len()).take(*height) {
                        out.push((i, k, [x + 32.0, ey + LINE * (k - *top) as f32, w - 64.0, LINE]));
                    }
                }
                _ => {}
            }
            ey += Self::classic_h(e);
        }
        out
    }

    /// The pointer on the classic page at (x, y): the cursor follows it over controls, a click acts. Whether it is
    /// over a control, and what it did.
    pub fn pointer_classic(&mut self, gu: &GameUi, x: f32, y: f32, p: (f32, f32), moved: bool, clicked: bool) -> (bool, Vec<Event>, Vec<Se>) {
        let inside = |r: &[f32; 4]| p.0 >= r[0] && p.1 >= r[1] && p.0 < r[0] + r[2] && p.1 < r[1] + r[3];
        let Some((i, k, r)) = self.classic_controls(gu, x, y).into_iter().find(|c| inside(&c.2)) else { return (false, Vec::new(), Vec::new()) };
        let mut se = Vec::new();
        let list = matches!(self.elems.get(i), Some(Elem::List { .. }));
        if (moved || clicked) && !self.edit && (self.at != i || (!list && self.col != k)) {
            (self.at, self.col) = (i, if list { 0 } else { k });
            se.push(Se::Cursor);
        }
        if list
            && let Some(Elem::List { at, .. }) = self.elems.get_mut(i)
            && (moved || clicked)
        {
            *at = k;
        }
        if !clicked {
            return (true, Vec::new(), se);
        }
        let (ev, more) = match self.elems.get(i) {
            Some(Elem::Slider { n, .. }) => {
                let stop = (((p.0 - r[0]) / r[2]).clamp(0.0, 1.0) * (*n - 1) as f32).round() as usize;
                self.slide(i, stop)
            }
            Some(Elem::List { .. }) => self.pick(i, k),
            _ => self.click_button(i, k),
        };
        se.extend(more);
        (true, ev, se)
    }

    /// Each element's height on the classic page.
    fn classic_h(e: &Elem) -> f32 {
        match e {
            Elem::Text { .. } => LINE,
            Elem::Buttons { .. } | Elem::Slider { .. } => PITCH,
            Elem::List { height, .. } => LINE * *height as f32 + 6.0,
            Elem::Gauge { .. } => LINE + 2.0,
            Elem::Separator => 8.0,
        }
    }

    pub fn classic_size(&self) -> (f32, f32) {
        let w = if self.width > 0.0 { self.width } else { CLASSIC_W };
        (w, TOP + self.elems.iter().map(Self::classic_h).sum::<f32>() + 12.0)
    }

    /// The classic page at (x, y): the Config pages' frame, bullets, buttons, sliders and the hand.
    pub fn draw_classic(&self, gu: &mut GameUi, x: f32, y: f32) -> [f32; 4] {
        const N: u32 = 0x8080_8080;
        const ON: u32 = 0x8080_5020;
        const DIM: u32 = 0x8060_6060;
        let (w, h) = self.classic_size();
        if self.framed {
            gu.frame(x, y, w, h);
        }
        gu.text(&self.title, x + 16.0, y + 10.0, 0, [0x80; 4]);
        let mut ey = y + TOP;
        let mut hand = None;
        for (i, e) in self.elems.iter().enumerate() {
            let here = self.focus && i == self.at;
            match e {
                Elem::Text { text, dim, .. } => {
                    gu.text_fit(text, x + 20.0, ey, w - 40.0, if *dim { DIM } else { N });
                }
                Elem::Buttons { label, items, marked, .. } => {
                    if let Some(b) = gu.cell_bounds("menu    windowps", 465, 2) {
                        gu.sprite_cells("menu    windowps", 465, 2..3, x + 20.0 - b[0], ey + 2.0 - b[1], N);
                    }
                    gu.label(label, x + 32.0, ey, N, false);
                    let need = items.iter().map(|s| gu.label_width(s) + 12.0).fold(0.0, f32::max).ceil();
                    let bw = if need <= 64.0 && items.len() >= 3 { 64.0 } else if need <= 88.0 { 88.0 } else { need };
                    for (c, s) in items.iter().enumerate() {
                        let bx = x + 32.0 + (bw + 8.0) * c as f32;
                        let by = ey + 14.0;
                        let on = here && c == self.col && !self.edit;
                        let d = if on { -1.0 } else { 0.0 };
                        super::super::settings::button(gu, [bx + d, by + d, bw, 16.0], if on { ON } else { N });
                        gu.label(s, bx + 5.0 + d, by + 1.0 + d, if on { ON } else { N }, true);
                        if *marked == Some(c) {
                            super::super::settings::marker(gu, bx, by, bw);
                        }
                        if on {
                            hand = Some((bx, by));
                        }
                    }
                }
                Elem::Slider { label, n, at, shown, .. } => {
                    if let Some(b) = gu.cell_bounds("menu    windowps", 465, 2) {
                        gu.sprite_cells("menu    windowps", 465, 2..3, x + 20.0 - b[0], ey + 2.0 - b[1], N);
                    }
                    gu.label(label, x + 32.0, ey, N, false);
                    let sw = (w - SLIDER_X - 48.0).max(96.0);
                    let vw = gu.label_width(shown);
                    gu.label(shown, x + SLIDER_X + sw - vw, ey, N, false);
                    let (sx, sy) = (x + SLIDER_X, ey + 14.0);
                    gu.sprite_cells("menu    windowps", 465, 8..11, sx - 88.0, sy - 80.0, N);
                    let f = if *n > 1 { *at as f32 / (*n - 1) as f32 } else { 1.0 };
                    if f > 0.0 {
                        gu.sprite_scaled("menu    windowps", 384, sx, sy, f, 1.0, if self.edit && here { 0xa0a0_a0a0 } else { N });
                    }
                    if here && !self.edit {
                        hand = Some((sx, sy));
                    }
                }
                Elem::List { rows, at, top, height, .. } => {
                    for (k, row) in rows.iter().enumerate().skip(*top).take(*height) {
                        let ry = ey + LINE * (k - *top) as f32;
                        gu.text_fit(row, x + 32.0, ry, w - 64.0, N);
                        if here && k == *at {
                            hand = Some((x + 32.0, ry));
                        }
                    }
                    if rows.len() > *height {
                        gu.scroll_bar(x + w - 20.0, ey, LINE * *height as f32, rows.len(), *height, *top);
                    }
                }
                Elem::Gauge { label, value, text, kind, .. } => {
                    // The Timers window's gauge: frame 38 and its fill 97, coloured by kind.
                    let colour = match kind.as_str() {
                        "mp" => [0x80, 0x50, 0x30, 0x80],
                        "tp" => [0x30, 0x60, 0x80, 0x80],
                        "cast" => [0x30, 0x80, 0x80, 0x80],
                        "experience" => [0x80, 0x40, 0x80, 0x80],
                        "recast" => [0x60, 0x60, 0x60, 0x80],
                        _ => [0x30, 0x80, 0x30, 0x80],
                    };
                    let gx = x + w - 92.0;
                    gu.text_fit(label, x + 20.0, ey, gx - x - 80.0, N);
                    gu.shape_text(text, gx - 8.0, ey + 1.0, N, true);
                    let f = value.clamp(0.0, 1.0);
                    if f > 0.0 {
                        gu.sprite_coloured("menu    windowps", 97, gx + 4.0, ey + 7.0, f, 1.0, colour, 8);
                    }
                    gu.sprite("menu    windowps", 38, gx, ey + 6.0, N);
                }
                Elem::Separator => {}
            }
            ey += Self::classic_h(e);
        }
        if let Some((hx, hy)) = hand {
            gu.cursor(hx, hy);
        }
        [x, y, w, h]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::Key as K;

    fn win() -> Window {
        let mut w = Window::new("w");
        w.elems = vec![
            Elem::Text { id: String::new(), text: "Hello".into(), dim: false },
            Elem::Buttons { id: "go".into(), label: "Go".into(), items: vec!["A".into(), "B".into()], marked: None },
            Elem::Buttons { id: "pick".into(), label: "Pick".into(), items: vec!["X".into(), "Y".into()], marked: Some(0) },
            Elem::Slider { id: "s".into(), label: "S".into(), n: 3, at: 0, shown: "0".into() },
            Elem::List { id: "l".into(), rows: vec!["r1".into(), "r2".into(), "r3".into()], at: 0, top: 0, height: 2 },
        ];
        w.open = true;
        w.focus = true;
        w
    }

    #[test]
    fn keys() {
        let mut w = win();
        // The first stop is the Go buttons.
        w.key(K::ArrowRight);
        assert_eq!(w.key(K::Enter).0, vec![Event::Press { id: "go".into(), index: 1 }]);
        // Options set once.
        w.key(K::ArrowDown);
        w.key(K::ArrowRight);
        assert_eq!(w.key(K::Enter).0, vec![Event::Set { id: "pick".into(), index: 1 }]);
        assert!(w.key(K::Enter).0.is_empty());
        // The slider edits after Enter.
        w.key(K::ArrowDown);
        w.key(K::Enter);
        assert_eq!(w.key(K::ArrowRight).0, vec![Event::Set { id: "s".into(), index: 1 }]);
        w.key(K::Enter);
        // The list moves inside, scrolls, and selects.
        w.key(K::ArrowDown);
        w.key(K::ArrowDown);
        w.key(K::ArrowDown);
        let Elem::List { at, top, .. } = &w.elems[4] else { panic!() };
        assert_eq!((*at, *top), (2, 1));
        assert_eq!(w.key(K::Enter).0, vec![Event::Select { id: "l".into(), index: 2 }]);
        // Past its end the cursor wraps to the first stop.
        w.key(K::ArrowDown);
        assert_eq!(w.at, 1);
        assert_eq!(w.key(K::Escape).0, vec![Event::Close]);
        assert!(!w.open);
    }
}
