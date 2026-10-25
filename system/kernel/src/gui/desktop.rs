//! The desktop: top bar, widgets, dock, launcher and windows.

use super::apps::{self, Launch};
use super::gfx::{Canvas, Rect};
use super::res;
use super::settings;
use super::splash::draw_logo;
use super::text::{self, Font};
use super::ui::{self, Event, Looper, View, TEXT, TEXT_DIM, TEXT_FAINT};
use super::wm::{Action, Hit, Window, TITLE_H};
use super::Display;
use crate::dev::input::{self, Key, KeyEvent};
use crate::time::{self, DateTime};
use crate::util::Size;
use alloc::boxed::Box;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

const WEEKDAYS: [&str; 7] = ["Lunedì", "Martedì", "Mercoledì", "Giovedì", "Venerdì", "Sabato", "Domenica"];
const WEEKDAYS_SHORT: [&str; 7] = ["lun", "mar", "mer", "gio", "ven", "sab", "dom"];
const MONTHS: [&str; 12] =
    ["gennaio", "febbraio", "marzo", "aprile", "maggio", "giugno", "luglio", "agosto", "settembre", "ottobre", "novembre", "dicembre"];
const MONTHS_SHORT: [&str; 12] = ["gen", "feb", "mar", "apr", "mag", "giu", "lug", "ago", "set", "ott", "nov", "dic"];

#[derive(Clone)]
enum Do {
    App(&'static str),
    Run(&'static str),
    Launcher,
}

const DOCK: [(&str, &str, Do); 8] = [
    ("files", "File", Do::App("files")),
    ("terminal", "Terminale", Do::App("terminal")),
    ("text-editor", "Editor di testo", Do::Run("edit")),
    ("store", "Negozio", Do::App("store")),
    ("settings", "Impostazioni", Do::App("settings")),
    ("info", "Info sistema", Do::App("about")),
    ("apps", "Applicazioni", Do::Launcher),
    ("trash", "Cestino", Do::App("trash")),
];
/// The dock has a separator before this item.
const DOCK_SPLIT: usize = 6;
const ICON: i32 = 52;
const DOCK_GAP: i32 = 12;
const DOCK_PAD: i32 = 12;

#[derive(Clone, Copy)]
enum MenuAct {
    Restart,
    PowerOff,
    Terminal,
    Files,
    Settings,
    NextWallpaper,
}

struct Menu {
    rect: Rect,
    items: Vec<(&'static str, MenuAct)>,
}

impl Menu {
    fn new(x: i32, y: i32, items: Vec<(&'static str, MenuAct)>, screen_w: i32) -> Menu {
        let w = 210;
        let h = items.len() as i32 * 36 + 12;
        Menu { rect: Rect::new(x.min(screen_w - w - 10), y, w, h), items }
    }

    fn item(&self, i: usize) -> Rect {
        Rect::new(self.rect.x + 6, self.rect.y + 6 + i as i32 * 36, self.rect.w - 12, 34)
    }
}

struct Launcher {
    query: String,
    entries: Vec<apps::Entry>,
}

impl Launcher {
    fn visible(&self) -> Vec<&apps::Entry> {
        let q = self.query.to_lowercase();
        self.entries.iter().filter(|e| q.is_empty() || e.name.to_lowercase().contains(&q)).collect()
    }
}

struct Desktop {
    w: i32,
    h: i32,
    sharp: Canvas,
    blur: Canvas,
    windows: Vec<Window>,
    next_id: u32,
    drag: Option<(u32, i32, i32)>,
    launcher: Option<Launcher>,
    menu: Option<Menu>,
    hover: Option<usize>,
    mouse: (i32, i32),
    minute: u64,
    second: u64,
    cascade: i32,
}

fn now() -> DateTime {
    time::from_epoch(time::now())
}

impl Desktop {
    fn topbar(&self) -> Rect {
        Rect::new(14, 10, self.w - 28, 40)
    }

    fn power_btn(&self) -> Rect {
        let t = self.topbar();
        Rect::new(t.right() - 40, t.y + 6, 28, 28)
    }

    fn widgets(&self) -> [Rect; 3] {
        let w = 300;
        [Rect::new(18, 66, w, 150), Rect::new(18, 228, w, 222), Rect::new(18, 462, w, 120)]
    }

    fn show_widgets(&self) -> usize {
        if self.h >= 700 {
            3
        } else if self.h >= 560 {
            2
        } else {
            0
        }
    }

    fn dock(&self) -> Rect {
        let n = DOCK.len() as i32;
        let w = n * ICON + (n - 1) * DOCK_GAP + 2 * DOCK_PAD + 18;
        let h = ICON + 2 * DOCK_PAD + 6;
        Rect::new((self.w - w) / 2, self.h - h - 12, w, h)
    }

    fn dock_item(&self, i: usize) -> Rect {
        let d = self.dock();
        let extra = if i >= DOCK_SPLIT { 18 } else { 0 };
        Rect::new(d.x + DOCK_PAD + i as i32 * (ICON + DOCK_GAP) + extra, d.y + DOCK_PAD, ICON, ICON)
    }

    fn work_area(&self) -> Rect {
        let top = self.topbar().bottom() + 8;
        Rect::new(8, top, self.w - 16, self.dock().y - top - 8)
    }

    fn screen(&self) -> Rect {
        Rect::new(0, 0, self.w, self.h)
    }

    fn window_mut(&mut self, id: u32) -> Option<&mut Window> {
        self.windows.iter_mut().find(|w| w.id == id)
    }

    fn focused(&self) -> Option<u32> {
        self.windows.iter().rev().find(|w| !w.minimized).map(|w| w.id)
    }

    fn raise(&mut self, l: &mut Looper, id: u32) {
        if let Some(i) = self.windows.iter().position(|w| w.id == id) {
            let mut win = self.windows.remove(i);
            win.minimized = false;
            l.damage(win.bounds());
            self.windows.push(win);
            // The window that lost focus redraws its title.
            if let Some(prev) = self.windows.iter().rev().nth(1) {
                l.damage(prev.bounds());
            }
        }
    }

    fn open(&mut self, l: &mut Looper, app: Box<dyn super::wm::App>, single: bool) -> u32 {
        if single {
            if let Some(id) = self.windows.iter().find(|w| w.app.kind() == app.kind()).map(|w| w.id) {
                self.raise(l, id);
                return id;
            }
        }
        let area = self.work_area();
        let (aw, ah) = app.size();
        let full_h = ah + TITLE_H + 8;
        let x = (area.x + (area.w - aw) / 2 + 150 + self.cascade * 26).min(area.right() - aw).max(area.x);
        let y = (area.y + 20 + self.cascade * 24).min(area.bottom() - full_h).max(area.y);
        self.cascade = (self.cascade + 1) % 6;
        let id = self.next_id;
        self.next_id += 1;
        let win = Window::new(id, app, x, y);
        l.damage(win.bounds());
        l.damage(self.dock().grow(10));
        if let Some(prev) = self.windows.last() {
            l.damage(prev.bounds());
        }
        self.windows.push(win);
        id
    }

    fn open_id(&mut self, l: &mut Looper, id: &'static str) {
        if let Some(app) = apps::open(id) {
            let single = id != "files";
            self.open(l, app, single);
        }
    }

    /// Types a command line into the terminal (opening it if needed).
    fn run(&mut self, l: &mut Looper, cmd: &str) {
        self.open_id(l, "terminal");
        let plain = |key| KeyEvent { key, ctrl: false, alt: false, shift: false };
        for ch in cmd.chars() {
            input::inject(plain(Key::Char(ch)));
        }
        input::inject(plain(Key::Enter));
    }

    fn act(&mut self, l: &mut Looper, from: Option<u32>, action: Action) {
        let damage_window = |s: &mut Desktop, l: &mut Looper| {
            if let Some(w) = from.and_then(|id| s.windows.iter().find(|w| w.id == id)) {
                l.damage(w.bounds());
            }
        };
        match action {
            Action::None => {}
            Action::Redraw => damage_window(self, l),
            Action::Run(cmd) => {
                damage_window(self, l);
                self.run(l, &cmd);
            }
            Action::View(path) => {
                self.open(l, Box::new(apps::Viewer::new(&path)), false);
            }
            Action::Wallpaper => {
                let (s, b) = res::wallpaper(&settings::get().wallpaper, self.w, self.h);
                self.sharp = s;
                self.blur = b;
                l.damage(self.screen());
            }
        }
    }

    fn dock_do(&mut self, l: &mut Looper, what: Do) {
        match what {
            Do::App(id) => self.open_id(l, id),
            Do::Run(cmd) => self.run(l, cmd),
            Do::Launcher => self.toggle_launcher(l),
        }
    }

    fn toggle_launcher(&mut self, l: &mut Looper) {
        self.launcher = match self.launcher {
            Some(_) => None,
            None => Some(Launcher { query: String::new(), entries: apps::catalog() }),
        };
        self.menu = None;
        l.damage(self.screen());
    }

    fn launch(&mut self, l: &mut Looper, e: Launch) {
        self.launcher = None;
        l.damage(self.screen());
        match e {
            Launch::App(id) => self.open_id(l, id),
            Launch::Run(cmd) => self.run(l, &cmd),
        }
    }

    fn launcher_tiles(&self, count: usize) -> Vec<Rect> {
        let cols = 6.min(((self.w - 80) / 128) as usize).max(1);
        let (tw, th) = (128, 116);
        let total_w = cols as i32 * tw;
        let x0 = (self.w - total_w) / 2;
        (0..count).map(|i| Rect::new(x0 + (i % cols) as i32 * tw, 150 + (i / cols) as i32 * th, tw, th)).collect()
    }

    fn search_rect(&self) -> Rect {
        Rect::new(self.w / 2 - 220, 70, 440, 46)
    }

    fn menu_act(&mut self, l: &mut Looper, act: MenuAct) {
        match act {
            MenuAct::Restart => {
                crate::fs::sync_all();
                crate::acpi::reboot();
            }
            MenuAct::PowerOff => {
                crate::fs::sync_all();
                crate::acpi::power_off();
            }
            MenuAct::Terminal => self.open_id(l, "terminal"),
            MenuAct::Files => self.open_id(l, "files"),
            MenuAct::Settings => self.open_id(l, "settings"),
            MenuAct::NextWallpaper => {
                let cur = settings::get().wallpaper;
                let i = res::WALLPAPERS.iter().position(|(n, _)| *n == cur).unwrap_or(0);
                let next = res::WALLPAPERS[(i + 1) % res::WALLPAPERS.len()].0;
                settings::update(|s| s.wallpaper = String::from(next));
                self.act(l, None, Action::Wallpaper);
            }
        }
    }

    fn press(&mut self, l: &mut Looper, x: i32, y: i32, double: bool) {
        if let Some(menu) = self.menu.take() {
            l.damage(ui::with_shadow(menu.rect));
            if let Some(i) = (0..menu.items.len()).find(|&i| menu.item(i).contains(x, y)) {
                self.menu_act(l, menu.items[i].1);
            }
            return;
        }
        if let Some(launcher) = &self.launcher {
            let visible: Vec<Launch> = launcher.visible().iter().map(|e| e.launch.clone()).collect();
            let tiles = self.launcher_tiles(visible.len());
            if let Some(i) = tiles.iter().position(|t| t.contains(x, y)) {
                self.launch(l, visible[i].clone());
            } else if !self.search_rect().contains(x, y) {
                self.launcher = None;
                l.damage(self.screen());
            }
            return;
        }
        if self.power_btn().contains(x, y) {
            let t = self.topbar();
            self.menu = Some(Menu::new(
                t.right() - 210,
                t.bottom() + 6,
                alloc::vec![("Riavvia", MenuAct::Restart), ("Spegni", MenuAct::PowerOff)],
                self.w,
            ));
            l.damage(self.screen());
            return;
        }
        if self.dock().contains(x, y) {
            if let Some(i) = (0..DOCK.len()).find(|&i| self.dock_item(i).grow(6).contains(x, y)) {
                self.dock_do(l, DOCK[i].2.clone());
            }
            return;
        }
        // Windows, the top one first.
        let hit = self.windows.iter().enumerate().rev().filter(|(_, w)| !w.minimized).find_map(|(i, w)| w.hit(x, y).map(|h| (i, w.id, h)));
        if let Some((_, id, hit)) = hit {
            if self.focused() != Some(id) {
                self.raise(l, id);
            }
            let screen = self.work_area();
            let Some(win) = self.window_mut(id) else { return };
            match hit {
                Hit::Close => {
                    l.damage(win.bounds());
                    self.windows.retain(|w| w.id != id);
                    l.damage(self.dock().grow(10));
                }
                Hit::Minimize => {
                    l.damage(win.bounds());
                    win.minimized = true;
                    l.damage(self.dock());
                }
                Hit::Maximize => {
                    l.damage(win.bounds());
                    win.toggle_maximize(screen);
                    l.damage(win.bounds());
                }
                Hit::Title => {
                    if double {
                        l.damage(win.bounds());
                        win.toggle_maximize(screen);
                        l.damage(win.bounds());
                    } else {
                        self.drag = Some((id, x - win.rect.x, y - win.rect.y));
                    }
                }
                Hit::Body(bx, by) => {
                    let body = win.body();
                    let action = win.app.press(bx, by, double, body);
                    self.act(l, Some(id), action);
                }
            }
        }
    }

    fn key(&mut self, l: &mut Looper, k: KeyEvent) {
        if k.ctrl && k.key == Key::Char(' ') {
            self.toggle_launcher(l);
            return;
        }
        if self.menu.is_some() && k.key == Key::Escape {
            if let Some(m) = self.menu.take() {
                l.damage(ui::with_shadow(m.rect));
            }
            return;
        }
        if let Some(launcher) = &mut self.launcher {
            match k.key {
                Key::Escape => {
                    self.launcher = None;
                }
                Key::Enter => {
                    if let Some(first) = launcher.visible().first().map(|e| e.launch.clone()) {
                        self.launch(l, first);
                    }
                }
                _ => {
                    ui::edit_text(&mut launcher.query, &k, 40);
                }
            }
            l.damage(self.screen());
            return;
        }
        if let Some(id) = self.focused() {
            if let Some(win) = self.window_mut(id) {
                let action = win.app.key(k);
                self.act(l, Some(id), action);
            }
        }
    }

    // ------------------------------------------------------------------
    // Drawing

    fn draw_topbar(&self, c: &mut Canvas) {
        let t = self.topbar();
        ui::glass(c, &self.blur, t, 16, ui::GLASS);
        draw_logo(c, t.x + 26, t.y + 17, 9, 0xFFFF_FFFF);
        text::draw(c, Font::Strong, t.x + 42, t.y + 11, "arcOS", TEXT);
        let d = now();
        let clock = format!("{:02}:{:02}", d.hour, d.min);
        let date = format!("{} {} {}", WEEKDAYS_SHORT[d.weekday as usize % 7], d.day, MONTHS_SHORT[(d.month as usize + 11) % 12]);
        let cw = text::width(Font::Strong, &clock) + 10 + text::width(Font::Body, &date);
        let x = t.x + (t.w - cw) / 2;
        let w = text::draw(c, Font::Strong, x, t.y + 11, &clock, TEXT);
        text::draw(c, Font::Body, x + w + 10, t.y + 11, &date, TEXT_DIM);
        // Status: network, then the power button.
        let mut sx = self.power_btn().x - 30;
        if crate::net::online() {
            res::draw_icon(c, "g-wifi", 18, sx, t.y + 11);
            sx -= 28;
        }
        res::draw_icon(c, "g-volume", 18, sx, t.y + 11);
        let p = self.power_btn();
        c.fill_circle(p.x + 14, p.y + 14, 14, 0x26FF_FFFF);
        res::draw_icon(c, "g-power", 16, p.x + 6, p.y + 6);
    }

    fn draw_widgets(&self, c: &mut Canvas, r: Rect) {
        let n = self.show_widgets();
        let ws = self.widgets();
        let d = now();
        let name = settings::get().name;
        if n > 0 && r.overlaps(&ui::with_shadow(ws[0])) {
            let g = ws[0];
            ui::glass(c, &self.blur, g, 22, ui::GLASS);
            let hello = match d.hour {
                5..=11 => "Buongiorno",
                12..=17 => "Buon pomeriggio",
                _ => "Buonasera",
            };
            let line = if name.is_empty() { String::from(hello) } else { format!("{hello}, {name}") };
            text::draw(c, Font::Body, g.x + 22, g.y + 18, &text::fit(Font::Body, &line, g.w - 40), TEXT_DIM);
            text::draw(c, Font::Clock, g.x + 18, g.y + 36, &format!("{:02}:{:02}", d.hour, d.min), TEXT);
            let date = format!("{} {} {}", WEEKDAYS[d.weekday as usize % 7], d.day, MONTHS[(d.month as usize + 11) % 12]);
            text::draw(c, Font::Body, g.x + 22, g.y + 116, &date, TEXT);
        }
        if n > 1 && r.overlaps(&ui::with_shadow(ws[1])) {
            let g = ws[1];
            ui::glass(c, &self.blur, g, 22, ui::GLASS);
            let title = format!("{} {}", capitalize(MONTHS[(d.month as usize + 11) % 12]), d.year);
            text::draw(c, Font::Strong, g.x + 20, g.y + 16, &title, TEXT);
            let cell = (g.w - 30) / 7;
            for (i, h) in ["L", "M", "M", "G", "V", "S", "D"].iter().enumerate() {
                text::draw_centered(c, Font::Small, g.x + 15 + i as i32 * cell + cell / 2, g.y + 46, h, TEXT_FAINT);
            }
            let first = time::from_epoch(time::to_epoch(d.year, d.month, 1, 12, 0, 0)).weekday as i32;
            let days = time::days_in_month(d.year, d.month) as i32;
            for day in 1..=days {
                let slot = first + day - 1;
                let (col, row) = (slot % 7, slot / 7);
                let cx = g.x + 15 + col * cell + cell / 2;
                let cy = g.y + 82 + row * 26;
                let label = format!("{day}");
                if day as u64 == d.day {
                    c.fill_circle(cx, cy, 12, 0xFFFF_FFFF);
                    text::draw_centered(c, Font::Strong, cx, cy - 9, &label, 0xFF1E_1626);
                } else {
                    text::draw_centered(c, Font::Body, cx, cy - 9, &label, if col >= 5 { TEXT_DIM } else { TEXT });
                }
            }
        }
        if n > 2 && r.overlaps(&ui::with_shadow(ws[2])) {
            let g = ws[2];
            ui::glass(c, &self.blur, g, 22, ui::GLASS);
            text::draw(c, Font::Strong, g.x + 20, g.y + 16, "Sistema", TEXT);
            let (free, total) = crate::mem::frame::stats();
            let used = total.saturating_sub(free);
            let bar = Rect::new(g.x + 20, g.y + 50, g.w - 40, 8);
            c.fill_round(bar, 4, 0x33FF_FFFF);
            let fw = (bar.w as u64 * used).checked_div(total).unwrap_or(0) as i32;
            c.fill_round(Rect::new(bar.x, bar.y, fw.max(8), bar.h), 4, 0xFFFF_FFFF);
            text::draw(c, Font::Small, g.x + 20, g.y + 66, &format!("Memoria: {} in uso su {}", Size(used), Size(total)), TEXT_DIM);
            let up = time::uptime_ms() / 60_000;
            let net = if crate::net::online() { "connessa" } else { "assente" };
            text::draw(c, Font::Small, g.x + 20, g.y + 88, &format!("Acceso da {} h {} min · rete {net}", up / 60, up % 60), TEXT_DIM);
        }
    }

    fn draw_dock(&self, c: &mut Canvas) {
        let d = self.dock();
        ui::glass(c, &self.blur, d, 24, ui::GLASS);
        let sep = self.dock_item(DOCK_SPLIT).x - DOCK_GAP / 2 - 9;
        c.fill(Rect::new(sep, d.y + 18, 1, d.h - 36), 0x40FF_FFFF);
        for (i, (icon, _, what)) in DOCK.iter().enumerate() {
            let r = self.dock_item(i);
            let lift = if self.hover == Some(i) { 4 } else { 0 };
            res::draw_icon(c, icon, ICON, r.x, r.y - lift);
            let open = match what {
                Do::App(id) => {
                    let kind = match *id {
                        "about" => "about",
                        "trash" => "files",
                        k => k,
                    };
                    *id != "trash" && self.windows.iter().any(|w| w.app.kind() == kind)
                }
                _ => false,
            };
            if open {
                c.fill_circle(r.x + ICON / 2, r.bottom() + 7, 2, 0xE6FF_FFFF);
            }
        }
        if let Some(i) = self.hover {
            let r = self.dock_item(i);
            let label = DOCK[i].1;
            let w = text::width(Font::Strong, label) + 24;
            let tip = Rect::new(r.x + r.w / 2 - w / 2, d.y - 42, w, 30);
            ui::glass(c, &self.blur, tip, 15, ui::GLASS_STRONG);
            text::draw_centered(c, Font::Strong, tip.x + w / 2, tip.y + 6, label, TEXT);
        }
    }

    fn draw_launcher(&self, c: &mut Canvas, launcher: &Launcher) {
        c.blit(&self.blur, 0, 0, self.screen());
        c.fill(self.screen(), 0x8C10_0A16);
        let s = self.search_rect();
        c.fill_round(s, 23, 0x33FF_FFFF);
        c.stroke_round(s, 23, 1, ui::BORDER);
        res::draw_icon(c, "g-search", 18, s.x + 18, s.y + 14);
        let (lh, _) = text::metrics(Font::Heading);
        if launcher.query.is_empty() {
            text::draw(c, Font::Heading, s.x + 48, s.y + (s.h - lh) / 2, "Cerca app…", TEXT_FAINT);
        } else {
            let w = text::draw(c, Font::Heading, s.x + 48, s.y + (s.h - lh) / 2, &launcher.query, TEXT);
            c.fill(Rect::new(s.x + 50 + w, s.y + 12, 2, s.h - 24), 0xFFFF_FFFF);
        }
        let visible = launcher.visible();
        if visible.is_empty() {
            text::draw_centered(c, Font::Body, self.w / 2, 180, "Nessuna app con questo nome", TEXT_DIM);
        }
        for (e, t) in visible.iter().zip(self.launcher_tiles(visible.len())) {
            if t.contains(self.mouse.0, self.mouse.1) {
                c.fill_round(t.grow(-6), 18, 0x22FF_FFFF);
            }
            res::draw_icon(c, e.icon, 64, t.x + (t.w - 64) / 2, t.y + 12);
            let label = text::fit(Font::Body, &e.name, t.w - 12);
            text::draw_centered(c, Font::Body, t.x + t.w / 2, t.y + 84, &label, TEXT);
        }
        text::draw_centered(c, Font::Small, self.w / 2, self.h - 40, "Ctrl+Spazio apre e chiude · Esc per uscire", TEXT_FAINT);
    }

    fn draw_menu(&self, c: &mut Canvas, m: &Menu) {
        ui::glass(c, &self.blur, m.rect, 14, ui::GLASS_STRONG);
        for (i, (label, _)) in m.items.iter().enumerate() {
            let r = m.item(i);
            if r.contains(self.mouse.0, self.mouse.1) {
                c.fill_round(r, 9, 0x2EFF_FFFF);
            }
            text::draw(c, Font::Body, r.x + 14, r.y + 8, label, TEXT);
        }
    }
}

fn capitalize(s: &str) -> String {
    let mut ch = s.chars();
    ch.next().map(|f| f.to_uppercase().chain(ch).collect()).unwrap_or_default()
}

impl View for Desktop {
    fn event(&mut self, l: &mut Looper, ev: Event) -> bool {
        match ev {
            Event::Press { x, y, double } => self.press(l, x, y, double),
            Event::Menu { x, y } => {
                if self.launcher.is_none()
                    && !self.windows.iter().any(|w| !w.minimized && w.rect.contains(x, y))
                    && !self.dock().contains(x, y)
                {
                    if let Some(m) = self.menu.take() {
                        l.damage(ui::with_shadow(m.rect));
                    }
                    let m = Menu::new(
                        x,
                        y,
                        alloc::vec![
                            ("Apri Terminale", MenuAct::Terminal),
                            ("Apri File", MenuAct::Files),
                            ("Cambia sfondo", MenuAct::NextWallpaper),
                            ("Impostazioni", MenuAct::Settings),
                        ],
                        self.w,
                    );
                    l.damage(ui::with_shadow(m.rect));
                    self.menu = Some(m);
                }
            }
            Event::Release => self.drag = None,
            Event::Move { x, y, held } => {
                self.mouse = (x, y);
                if let (true, Some((id, dx, dy))) = (held, self.drag) {
                    let area = self.screen();
                    if let Some(win) = self.window_mut(id) {
                        let before = win.bounds();
                        win.restore = None;
                        win.rect.x = (x - dx).clamp(area.x - win.rect.w + 80, area.right() - 80);
                        win.rect.y = (y - dy).clamp(area.y + 50, area.bottom() - 60);
                        let after = win.bounds();
                        l.damage(before.union(&after));
                    }
                    return false;
                }
                if let Some(m) = &self.menu {
                    l.damage(m.rect);
                }
                if self.launcher.is_some() {
                    l.damage(Rect::new(0, 140, self.w, self.h - 180));
                }
                let hover = if self.dock().grow(4).contains(x, y) && self.launcher.is_none() {
                    (0..DOCK.len()).find(|&i| self.dock_item(i).grow(6).contains(x, y))
                } else {
                    None
                };
                if hover != self.hover {
                    self.hover = hover;
                    let d = self.dock();
                    l.damage(Rect::new(d.x - 80, d.y - 60, d.w + 160, d.h + 60));
                }
            }
            Event::Wheel { lines } => {
                let (x, y) = self.mouse;
                if let Some(win) = self.windows.iter_mut().rev().find(|w| !w.minimized && w.body().contains(x, y)) {
                    if win.app.scroll(lines) {
                        l.damage(win.bounds());
                    }
                }
            }
            Event::Key(k) => self.key(l, k),
            Event::Tick => {
                let d = now();
                let minute = d.hour * 60 + d.min;
                if minute != self.minute {
                    self.minute = minute;
                    l.damage(ui::with_shadow(self.topbar()));
                    for w in self.widgets() {
                        l.damage(ui::with_shadow(w));
                    }
                }
                let second = time::uptime_ms() / 5000;
                if second != self.second {
                    self.second = second;
                    l.damage(ui::with_shadow(self.widgets()[2]));
                }
                for w in self.windows.iter_mut() {
                    if !w.minimized && w.app.tick() {
                        l.damage(w.body());
                    }
                }
            }
        }
        false
    }

    fn draw(&mut self, c: &mut Canvas, r: Rect) {
        if let Some(launcher) = &self.launcher {
            self.draw_launcher(c, launcher);
            return;
        }
        c.blit(&self.sharp, r.x, r.y, r);
        self.draw_widgets(c, r);
        let focused = self.focused();
        let blur = &self.blur;
        for w in self.windows.iter_mut() {
            if !w.minimized && r.overlaps(&w.bounds()) {
                let f = Some(w.id) == focused;
                w.draw(c, blur, f, r);
            }
        }
        if r.overlaps(&self.dock().grow(80)) {
            self.draw_dock(c);
        }
        if r.overlaps(&ui::with_shadow(self.topbar())) {
            self.draw_topbar(c);
        }
        if let Some(m) = &self.menu {
            self.draw_menu(c, m);
        }
    }
}

pub fn run(d: &mut Display) {
    let (w, h) = (d.width(), d.height());
    let (sharp, blur) = res::wallpaper(&settings::get().wallpaper, w, h);
    let mut desk = Desktop {
        w,
        h,
        sharp,
        blur,
        windows: Vec::new(),
        next_id: 1,
        drag: None,
        launcher: None,
        menu: None,
        hover: None,
        mouse: (-1, -1),
        minute: u64::MAX,
        second: 0,
        cascade: 0,
    };
    let mut l = Looper::new(d);
    l.damage_all(d);
    l.run(d, &mut desk);
}
