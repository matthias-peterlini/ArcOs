//! The desktop: top bar, widgets, dock, launcher, windows and the
//! animations between them.
//!
//! Everything that moves is driven by time ([`ui::Anim`]), not by frames:
//! on a slow machine an animation shows fewer frames but takes as long.
//! A window that opens, closes, shrinks into the dock or zooms is drawn once
//! into a picture of its own ([`Window::snapshot`]) and that picture is
//! scaled and faded on every frame, so the app is not redrawn meanwhile.

use super::apps::{self, Launch};
use super::gfx::{Canvas, Rect};
use super::res;
use super::settings;
use super::splash::draw_logo;
use super::text::{self, Font};
use super::ui::{self, Anim, Event, Glide, Looper, View, TEXT, TEXT_DIM, TEXT_FAINT};
use super::wm::{Action, App, Hit, Window, GAP, TITLE_H};
use super::Display;
use crate::dev::input::{self, Key, KeyEvent};
use crate::fs;
use crate::time::{self, DateTime};
use crate::util::Size;
use alloc::boxed::Box;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

const WEEKDAYS: [&str; 7] = ["Lunedì", "Martedì", "Mercoledì", "Giovedì", "Venerdì", "Sabato", "Domenica"];
const WEEKDAYS_SHORT: [&str; 7] = ["lun", "mar", "mer", "gio", "ven", "sab", "dom"];
pub const MONTHS: [&str; 12] =
    ["gennaio", "febbraio", "marzo", "aprile", "maggio", "giugno", "luglio", "agosto", "settembre", "ottobre", "novembre", "dicembre"];
const MONTHS_SHORT: [&str; 12] = ["gen", "feb", "mar", "apr", "mag", "giu", "lug", "ago", "set", "ott", "nov", "dic"];

/// The apps kept in the dock, in order (app id, icon, name).
const PINNED: [(&str, &str, &str); 10] = [
    ("files", "files", "File"),
    ("launcher", "apps", "Applicazioni"),
    ("textedit", "text-editor", "Editor di testo"),
    ("notes", "notes", "Note"),
    ("calendar", "calendar", "Calendario"),
    ("photos", "photos", "Foto"),
    ("music", "music", "Musica"),
    ("calculator", "calculator", "Calcolatrice"),
    ("store", "store", "Negozio"),
    ("settings", "settings", "Impostazioni"),
];
const ICON: i32 = 52;
/// How much bigger the icon under the pointer gets.
const MAGNIFY: i32 = 30;
const DOCK_GAP: i32 = 10;
const DOCK_PAD: i32 = 11;
const SEP_W: i32 = 19;

/// One icon in the dock.
#[derive(Clone)]
struct DockItem {
    /// The app id ("launcher" for the launcher, "trash").
    id: String,
    icon: String,
    label: String,
    /// The window kind it stands for.
    kind: String,
    /// What it starts when nothing of its kind is open.
    launch: Option<Launch>,
    /// A separator comes before it.
    split: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum MenuAct {
    About,
    Settings,
    Store,
    Restart,
    PowerOff,
    NextWallpaper,
    Display,
    Activity,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum PopupKind {
    Menu,
    /// The control centre: volume and status.
    Control,
}

struct Popup {
    kind: PopupKind,
    rect: Rect,
    items: Vec<(&'static str, Option<MenuAct>)>,
    anim: Anim,
}

impl Popup {
    fn menu(x: i32, y: i32, items: Vec<(&'static str, Option<MenuAct>)>, screen_w: i32) -> Popup {
        let w = 230;
        let h = items.iter().map(|(_, a)| if a.is_some() { 34 } else { 11 }).sum::<i32>() + 12;
        Popup { kind: PopupKind::Menu, rect: Rect::new(x.min(screen_w - w - 10).max(10), y, w, h), items, anim: Anim::new(140) }
    }

    /// The rectangle of each item that can be chosen.
    fn item_rects(&self) -> Vec<(Rect, usize)> {
        let mut y = self.rect.y + 6;
        let mut out = Vec::new();
        for (i, (_, act)) in self.items.iter().enumerate() {
            if act.is_some() {
                out.push((Rect::new(self.rect.x + 6, y, self.rect.w - 12, 32), i));
                y += 34;
            } else {
                y += 11;
            }
        }
        out
    }

    /// The part shown so far (popups unfold from the top).
    fn shown(&self) -> Rect {
        let t = self.anim.out();
        let r = ui::with_shadow(self.rect);
        Rect::new(r.x, r.y, r.w, ui::lerp(r.h / 3, r.h, t))
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum FxKind {
    Open,
    Close,
    Minimize,
    Restore,
    /// Maximise or back: from this rectangle (the window's old bounds).
    Zoom(Rect),
}

/// A window in motion: its picture and where it goes.
struct Fx {
    id: u32,
    kind: FxKind,
    anim: Anim,
    snap: Canvas,
    /// The window's bounds when at rest.
    at: Rect,
    /// The dock icon a minimised window shrinks into.
    dock: Rect,
    /// Where it was drawn last frame.
    last: Rect,
}

impl Fx {
    fn ms(kind: FxKind) -> u64 {
        match kind {
            FxKind::Open => 230,
            FxKind::Close => 170,
            FxKind::Minimize | FxKind::Restore => 340,
            FxKind::Zoom(_) => 230,
        }
    }

    /// Where the picture goes now, and how opaque it is.
    fn place(&self) -> (Rect, u32) {
        match self.kind {
            FxKind::Open => {
                let t = self.anim.out();
                (ui::scale_rect(self.at, ui::lerp(860, 1000, t)), t * 255 / 1000)
            }
            FxKind::Close => {
                let t = self.anim.out();
                (ui::scale_rect(self.at, ui::lerp(1000, 900, t)), (1000 - t) * 255 / 1000)
            }
            FxKind::Minimize => {
                let t = self.anim.in_out();
                (ui::lerp_rect(self.at, self.dock, t), 255 - t * 175 / 1000)
            }
            FxKind::Restore => {
                let t = self.anim.in_out();
                (ui::lerp_rect(self.dock, self.at, t), 80 + t * 175 / 1000)
            }
            FxKind::Zoom(from) => (ui::lerp_rect(from, self.at, self.anim.out()), 255),
        }
    }
}

/// A notification banner at the top right.
struct Note {
    title: String,
    body: String,
    icon: String,
    born: Anim,
}

const NOTE_W: i32 = 340;
const NOTE_H: i32 = 76;
const NOTE_IN: u64 = 320;
const NOTE_STAY: u64 = 4500;

impl Note {
    /// Slide offset 0..=1000 (1000: fully in).
    fn slide(&self) -> u32 {
        let e = self.born.elapsed();
        if e < NOTE_IN {
            ui::ease_out((e * 1000 / NOTE_IN) as u32)
        } else if e < NOTE_IN + NOTE_STAY {
            1000
        } else {
            let out = (e - NOTE_IN - NOTE_STAY).min(NOTE_IN);
            1000 - ui::ease_in_out((out * 1000 / NOTE_IN) as u32)
        }
    }

    fn moving(&self) -> bool {
        let e = self.born.elapsed();
        e < NOTE_IN || (NOTE_IN + NOTE_STAY..NOTE_IN * 2 + NOTE_STAY + 50).contains(&e)
    }

    fn gone(&self) -> bool {
        self.born.elapsed() > NOTE_IN * 2 + NOTE_STAY + 50
    }
}

enum Drag {
    Move(u32, i32, i32),
    /// Resizing: the window and the pointer's offset from its corner.
    Resize(u32, i32, i32),
    /// A press on a window's content: moves go to the app.
    Body(u32),
    Volume,
}

struct Launcher {
    query: String,
    entries: Vec<apps::Entry>,
    anim: Anim,
    closing: bool,
    /// What to start once it has faded out.
    then: Option<Launch>,
    scroll: i32,
}

impl Launcher {
    fn visible(&self) -> Vec<&apps::Entry> {
        let q = self.query.to_lowercase();
        self.entries.iter().filter(|e| q.is_empty() || e.name.to_lowercase().contains(&q)).collect()
    }

    /// How far it has faded in, 0..=1000.
    fn shown(&self) -> u32 {
        let t = self.anim.out();
        if self.closing {
            1000 - t
        } else {
            t
        }
    }
}

enum Ending {
    PowerOff,
    Restart,
}

struct Desktop {
    w: i32,
    h: i32,
    sharp: Canvas,
    blur: Canvas,
    windows: Vec<Window>,
    next_id: u32,
    drag: Option<Drag>,
    launcher: Option<Launcher>,
    popup: Option<Popup>,
    fx: Vec<Fx>,
    notes: Vec<Note>,
    /// The dock item under the pointer.
    hover: Option<usize>,
    /// The dock's magnification, 0..=1000.
    mag: Glide,
    /// An icon bouncing because its app is starting.
    bounce: Option<(String, Anim)>,
    mouse: (i32, i32),
    minute: u64,
    second: u64,
    cascade: i32,
    /// The window under the pointer that was told about it last.
    hovered: Option<u32>,
    ending: Option<(Ending, Anim)>,
    /// A full-screen window is in front (no bar, no dock).
    was_full: bool,
    /// Full screen, pointer at the bottom edge: the dock is shown.
    dock_reveal: bool,
}

fn now() -> DateTime {
    time::from_epoch(time::now())
}

impl Desktop {
    fn topbar(&self) -> Rect {
        Rect::new(14, 10, self.w - 28, 40)
    }

    fn logo_btn(&self) -> Rect {
        let t = self.topbar();
        Rect::new(t.x + 6, t.y + 4, 58, 32)
    }

    fn power_btn(&self) -> Rect {
        let t = self.topbar();
        Rect::new(t.right() - 40, t.y + 6, 28, 28)
    }

    fn control_btn(&self) -> Rect {
        let p = self.power_btn();
        Rect::new(p.x - 70, p.y, 62, 28)
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

    // ------------------------------------------------------------------
    // The dock

    fn dock_items(&self) -> Vec<DockItem> {
        let mut v: Vec<DockItem> = PINNED
            .iter()
            .map(|&(id, icon, label)| DockItem {
                id: id.into(),
                icon: icon.into(),
                label: label.into(),
                kind: apps::kind_of(id).into(),
                launch: if id == "launcher" { None } else { Some(Launch::App(id)) },
                split: false,
            })
            .collect();
        // Running apps that are not pinned, after a separator.
        let mut first = true;
        for w in &self.windows {
            let kind = w.app.kind();
            if kind == "trash" || v.iter().any(|d| d.kind == kind) {
                continue;
            }
            v.push(DockItem {
                id: String::new(),
                icon: w.app.icon().into(),
                label: apps::app_name(w.app.kind(), &w.app.title()),
                kind: kind.into(),
                launch: None,
                split: first,
            });
            first = false;
        }
        v.push(DockItem {
            id: "trash".into(),
            icon: "trash".into(),
            label: "Cestino".into(),
            kind: "trash".into(),
            launch: Some(Launch::App("trash")),
            split: true,
        });
        v
    }

    /// The dock's base layout, before magnification: (x of each icon, total width).
    fn dock_base(items: &[DockItem]) -> (Vec<i32>, i32) {
        let mut xs = Vec::new();
        let mut x = DOCK_PAD;
        for (i, it) in items.iter().enumerate() {
            if it.split && i > 0 {
                x += SEP_W - DOCK_GAP;
                x += DOCK_GAP;
            }
            xs.push(x);
            x += ICON + DOCK_GAP;
        }
        (xs, x - DOCK_GAP + DOCK_PAD)
    }

    /// The icons' rectangles on the screen, magnified around the pointer,
    /// and the dock's background.
    fn dock_layout(&self, items: &[DockItem]) -> (Vec<Rect>, Rect) {
        let (xs, total) = Self::dock_base(items);
        let dock_h = ICON + 2 * DOCK_PAD + 4;
        let base_x = (self.w - total) / 2;
        let bottom = self.h - 12 - DOCK_PAD - 4;
        let mag = self.mag.value.clamp(0, 1000) as i64;
        let range = (ICON + DOCK_GAP) as i64 * 5 / 2;
        let mx = self.mouse.0 as i64;
        let sizes: Vec<i32> = xs
            .iter()
            .map(|&x| {
                if mag == 0 {
                    return ICON;
                }
                let c = (base_x + x + ICON / 2) as i64;
                let d = (mx - c).abs();
                let f = if d >= range { 0 } else { 1000 - d * d * 1000 / (range * range) };
                ICON + (MAGNIFY as i64 * mag * f / 1_000_000) as i32
            })
            .collect();
        let extra: i32 = sizes.iter().map(|s| s - ICON).sum();
        let total_now = total + extra;
        let x0 = (self.w - total_now) / 2;
        let mut rects = Vec::new();
        let mut shift = 0;
        for (i, &x) in xs.iter().enumerate() {
            let s = sizes[i] & !1;
            rects.push(Rect::new(x0 + x + shift, bottom - s, s, s));
            shift += sizes[i] - ICON;
        }
        // Keep the point under the pointer where it was, so the icon being
        // aimed at does not slide away as the others grow.
        if mag > 0 && !xs.is_empty() {
            let k = xs.iter().rposition(|&x| base_x + x - DOCK_GAP / 2 <= self.mouse.0).unwrap_or(0);
            let f = ((self.mouse.0 - (base_x + xs[k])) as i64 * 1000 / ICON as i64).clamp(0, 1000);
            let now_at = rects[k].x as i64 + rects[k].w as i64 * f / 1000;
            let shift = ((mx - now_at) * mag / 1000) as i32;
            let shift = shift.clamp(-extra / 2, extra / 2);
            for r in rects.iter_mut() {
                r.x += shift;
            }
            let bg = Rect::new(x0 + shift, self.h - dock_h - 12, total_now, dock_h);
            return (rects, bg);
        }
        let bg = Rect::new(x0, self.h - dock_h - 12, total_now, dock_h);
        (rects, bg)
    }

    fn dock(&self) -> Rect {
        self.dock_layout(&self.dock_items()).1
    }

    /// The area the dock may paint (icons grow above it).
    fn dock_zone(&self) -> Rect {
        let d = self.dock();
        Rect::new(d.x - MAGNIFY * 4 - 30, d.y - MAGNIFY - 60, d.w + MAGNIFY * 8 + 60, d.h + MAGNIFY + 80)
    }

    /// The dock icon of a window kind (where it minimises to).
    fn dock_icon_of(&self, kind: &str) -> Rect {
        let items = self.dock_items();
        let (rects, bg) = self.dock_layout(&items);
        items.iter().position(|d| d.kind == kind).map(|i| rects[i]).unwrap_or(Rect::new(bg.x + bg.w / 2 - 20, bg.y, 40, 40))
    }

    fn work_area(&self) -> Rect {
        let top = self.topbar().bottom() + 8;
        let dock_top = self.h - (ICON + 2 * DOCK_PAD + 4) - 12;
        Rect::new(8, top, self.w - 16, dock_top - top - 8)
    }

    fn screen(&self) -> Rect {
        Rect::new(0, 0, self.w, self.h)
    }

    /// Where a maximised window goes: the whole screen, with a thin margin
    /// of wallpaper around the card.
    fn full_area(&self) -> Rect {
        Rect::new(10, 10, self.w - 20, self.h - 20)
    }

    /// Is a full-screen window in front? Then the bar and the dock hide.
    fn full_screen(&self) -> bool {
        self.launcher.is_none() && self.focused().and_then(|id| self.window(id)).is_some_and(|w| w.full())
    }

    /// Is the dock on the screen?
    fn dock_shown(&self) -> bool {
        !self.full_screen() || self.dock_reveal
    }

    // ------------------------------------------------------------------
    // Windows

    fn window_mut(&mut self, id: u32) -> Option<&mut Window> {
        self.windows.iter_mut().find(|w| w.id == id)
    }

    fn window(&self, id: u32) -> Option<&Window> {
        self.windows.iter().find(|w| w.id == id)
    }

    fn focused(&self) -> Option<u32> {
        self.windows.iter().rev().find(|w| !w.minimized).map(|w| w.id)
    }

    fn moving(&self, id: u32) -> bool {
        self.fx.iter().any(|f| f.id == id)
    }

    fn raise(&mut self, l: &mut Looper, id: u32) {
        if let Some(i) = self.windows.iter().position(|w| w.id == id) {
            let win = self.windows.remove(i);
            let was_min = win.minimized;
            l.damage(win.bounds());
            self.windows.push(win);
            if was_min {
                self.start_fx(l, id, FxKind::Restore);
                if let Some(w) = self.window_mut(id) {
                    w.minimized = false;
                }
            }
            // The window that lost focus redraws its title.
            if let Some(prev) = self.windows.iter().rev().nth(1) {
                l.damage(prev.bounds());
            }
            l.damage(ui::with_shadow(self.topbar()));
        }
    }

    /// Starts an animation of window `id` (it is drawn as a picture until
    /// the end).
    fn start_fx(&mut self, l: &mut Looper, id: u32, kind: FxKind) {
        self.fx.retain(|f| f.id != id || matches!(f.kind, FxKind::Close));
        let focused = self.focused() == Some(id) || kind == FxKind::Open;
        let blur = core::mem::replace(&mut self.blur, Canvas::new(0, 0, 0));
        let dock = self.window(id).map(|w| self.dock_icon_of(w.app.kind())).unwrap_or_default();
        let Some(win) = self.window_mut(id) else {
            self.blur = blur;
            return;
        };
        let (snap, at) = win.snapshot(&blur, focused);
        self.blur = blur;
        l.damage(at);
        self.fx.push(Fx { id, kind, anim: Anim::new(Fx::ms(kind)), snap, at, dock, last: at });
    }

    fn open(&mut self, l: &mut Looper, app: Box<dyn App>, single: bool) -> u32 {
        if single {
            if let Some(id) = self.windows.iter().rev().find(|w| w.app.kind() == app.kind()).map(|w| w.id) {
                self.raise(l, id);
                return id;
            }
        }
        let area = self.work_area();
        let (aw, ah) = app.size();
        let (aw, ah) = (aw.min(area.w), ah.min(area.h - TITLE_H - GAP));
        let full_h = ah + TITLE_H + GAP;
        let x = (area.x + (area.w - aw) / 2 + 120 + self.cascade * 26).min(area.right() - aw).max(area.x);
        let y = (area.y + 14 + self.cascade * 24).min(area.bottom() - full_h).max(area.y);
        self.cascade = (self.cascade + 1) % 6;
        let id = self.next_id;
        self.next_id += 1;
        let mut win = Window::new(id, app, x, y);
        win.rect.w = aw;
        win.rect.h = full_h;
        let kind = String::from(win.app.kind());
        if let Some(prev) = self.windows.last() {
            l.damage(prev.bounds());
        }
        self.windows.push(win);
        self.start_fx(l, id, FxKind::Open);
        l.damage(self.dock_zone());
        l.damage(ui::with_shadow(self.topbar()));
        self.bounce_icon(&kind);
        id
    }

    fn bounce_icon(&mut self, kind: &str) {
        self.bounce = Some((String::from(kind), Anim::new(650)));
    }

    fn open_id(&mut self, l: &mut Looper, id: &'static str) {
        if id == "launcher" {
            self.toggle_launcher(l);
            return;
        }
        if let Some(app) = apps::open(id) {
            let single = apps::single(id);
            self.open(l, app, single);
        }
    }

    fn close_window(&mut self, l: &mut Looper, id: u32) {
        if !self.windows.iter().any(|w| w.id == id) {
            return;
        }
        self.start_fx(l, id, FxKind::Close);
        if let Some(i) = self.windows.iter().position(|w| w.id == id) {
            let mut win = self.windows.remove(i);
            win.app.close();
            l.damage(win.bounds());
        }
        l.damage(self.dock_zone());
        l.damage(ui::with_shadow(self.topbar()));
        if let Some(top) = self.windows.last() {
            l.damage(top.bounds());
        }
    }

    fn minimize(&mut self, l: &mut Looper, id: u32) {
        self.start_fx(l, id, FxKind::Minimize);
        if let Some(top) = self.windows.iter().rev().filter(|w| w.id != id && !w.minimized).map(|w| w.bounds()).next() {
            l.damage(top);
        }
        l.damage(ui::with_shadow(self.topbar()));
    }

    fn zoom(&mut self, l: &mut Looper, id: u32) {
        let screen = self.full_area();
        let Some(win) = self.window_mut(id) else { return };
        let before = win.bounds();
        win.toggle_maximize(screen);
        l.damage(before);
        let after = win.bounds();
        l.damage(after);
        self.start_fx(l, id, FxKind::Zoom(before));
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

    fn notify(&mut self, l: &mut Looper, title: String, body: String, icon: String) {
        self.notes.retain(|n| !n.gone());
        if self.notes.len() >= 3 {
            self.notes.remove(0);
        }
        self.notes.push(Note { title, body, icon, born: Anim::new(1) });
        l.damage(self.notes_zone());
    }

    fn notes_zone(&self) -> Rect {
        let t = self.topbar();
        Rect::new(self.w - NOTE_W - 60, t.bottom(), NOTE_W + 60, 3 * (NOTE_H + 10) + 30)
    }

    fn note_rect(&self, i: usize, slide: u32) -> Rect {
        let t = self.topbar();
        let x_in = self.w - NOTE_W - 18;
        let x = ui::lerp(self.w + 20, x_in, slide);
        Rect::new(x, t.bottom() + 12 + i as i32 * (NOTE_H + 10), NOTE_W, NOTE_H)
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
            Action::OpenFile(path) => {
                damage_window(self, l);
                self.open_file(l, &path);
            }
            Action::Open(id) => {
                damage_window(self, l);
                self.open_id(l, id);
            }
            Action::Spawn(path) => {
                damage_window(self, l);
                self.spawn(l, &path);
            }
            Action::Notify(title, body, icon) => {
                damage_window(self, l);
                self.notify(l, title, body, String::from(icon));
            }
            Action::Wallpaper => {
                let (s, b) = res::wallpaper(&settings::get().wallpaper, self.w, self.h);
                self.sharp = s;
                self.blur = b;
                l.damage(self.screen());
            }
        }
    }

    fn open_file(&mut self, l: &mut Looper, path: &str) {
        match apps::for_file(path) {
            apps::Opener::App(app) => {
                self.open(l, app, false);
            }
            apps::Opener::Spawn(p) => self.spawn(l, &p),
            apps::Opener::Run(cmd) => self.run(l, &cmd),
            apps::Opener::None => {
                let name = String::from(path.rsplit('/').next().unwrap_or(path));
                self.notify(l, name, String::from("Nessuna app può aprire questo tipo di file."), String::from("file"));
            }
        }
    }

    /// Starts a program that opens a window of its own.
    fn spawn(&mut self, l: &mut Looper, path: &str) {
        let name = apps::program_name(path);
        let home = fs::abs("sys:/home").unwrap_or_else(|_| Path::root("sys"));
        match fs::abs(path).map_err(|_| arcdk::syscall::Error::NotFound).and_then(|p| crate::user::spawn(&p, "", home)) {
            Ok(_) => self.bounce = Some((format!("@{path}"), Anim::new(900))),
            Err(e) => self.notify(l, name, format!("Non si è potuto avviare: {}", e.as_str()), String::from("window")),
        }
    }

    fn launch(&mut self, l: &mut Looper, e: Launch) {
        match e {
            Launch::App(id) => self.open_id(l, id),
            Launch::Spawn(path) => self.spawn(l, &path),
            Launch::Run(cmd) => self.run(l, &cmd),
        }
    }

    // ------------------------------------------------------------------
    // The launcher

    fn toggle_launcher(&mut self, l: &mut Looper) {
        match &mut self.launcher {
            Some(la) if !la.closing => {
                la.closing = true;
                la.anim = Anim::new(200);
            }
            Some(la) => {
                la.closing = false;
                la.anim = Anim::new(260);
            }
            None => {
                self.launcher =
                    Some(Launcher { query: String::new(), entries: apps::catalog(), anim: Anim::new(260), closing: false, then: None, scroll: 0 });
            }
        }
        self.popup = None;
        l.damage(self.screen());
    }

    fn close_launcher(&mut self, l: &mut Looper, then: Option<Launch>) {
        if let Some(la) = &mut self.launcher {
            if !la.closing {
                la.closing = true;
                la.anim = Anim::new(200);
            }
            la.then = then;
        }
        l.damage(self.screen());
    }

    fn launcher_tiles(&self, count: usize, scroll: i32) -> Vec<Rect> {
        let cols = 7.min(((self.w - 80) / 128) as usize).max(1);
        let (tw, th) = (128, 120);
        let total_w = cols as i32 * tw;
        let x0 = (self.w - total_w) / 2;
        (0..count).map(|i| Rect::new(x0 + (i % cols) as i32 * tw, 150 + (i / cols) as i32 * th - scroll, tw, th)).collect()
    }

    fn search_rect(&self) -> Rect {
        Rect::new(self.w / 2 - 200, 74, 400, 40)
    }

    // ------------------------------------------------------------------
    // Menus and the control centre

    fn menu_act(&mut self, l: &mut Looper, act: MenuAct) {
        match act {
            MenuAct::Restart => self.end(l, Ending::Restart),
            MenuAct::PowerOff => self.end(l, Ending::PowerOff),
            MenuAct::About => self.open_id(l, "about"),
            MenuAct::Settings => self.open_id(l, "settings"),
            MenuAct::Display => self.open_id(l, "settings"),
            MenuAct::Store => self.open_id(l, "store"),
            MenuAct::Activity => self.open_id(l, "monitor"),
            MenuAct::NextWallpaper => {
                let cur = settings::get().wallpaper;
                let i = res::WALLPAPERS.iter().position(|(n, _)| *n == cur).unwrap_or(0);
                let next = res::WALLPAPERS[(i + 1) % res::WALLPAPERS.len()].0;
                settings::update(|s| s.wallpaper = String::from(next));
                self.act(l, None, Action::Wallpaper);
            }
        }
    }

    fn end(&mut self, l: &mut Looper, how: Ending) {
        crate::fs::sync_all();
        for w in self.windows.iter_mut() {
            w.app.close();
        }
        self.ending = Some((how, Anim::new(700)));
        l.damage(self.screen());
    }

    fn show_popup(&mut self, l: &mut Looper, p: Popup) {
        if let Some(old) = self.popup.take() {
            l.damage(ui::with_shadow(old.rect));
        }
        l.damage(ui::with_shadow(p.rect));
        self.popup = Some(p);
    }

    fn close_popup(&mut self, l: &mut Looper) {
        if let Some(p) = self.popup.take() {
            l.damage(ui::with_shadow(p.rect));
        }
    }

    fn control_rect(&self) -> Rect {
        let t = self.topbar();
        Rect::new(t.right() - 320, t.bottom() + 8, 310, 236)
    }

    fn volume_slider(&self) -> Rect {
        let r = self.control_rect();
        Rect::new(r.x + 58, r.y + 52, r.w - 84, 24)
    }

    fn control_settings_btn(&self) -> Rect {
        let r = self.control_rect();
        Rect::new(r.x + 16, r.bottom() - 52, r.w - 32, 36)
    }

    fn set_volume_at(&mut self, l: &mut Looper, x: i32) {
        let v = ui::slider_value(self.volume_slider(), x) / 10;
        super::change_volume(v);
        l.damage(ui::with_shadow(self.control_rect()));
        l.damage(ui::with_shadow(self.topbar()));
    }

    // ------------------------------------------------------------------
    // Input

    fn press(&mut self, l: &mut Looper, x: i32, y: i32, double: bool) {
        if self.ending.is_some() {
            return;
        }
        // Notifications go away when clicked.
        for i in (0..self.notes.len()).rev() {
            let r = self.note_rect(i, self.notes[i].slide());
            if r.contains(x, y) {
                l.damage(ui::with_shadow(r));
                self.notes.remove(i);
                l.damage(self.notes_zone());
                return;
            }
        }
        if let Some(p) = &self.popup {
            if p.kind == PopupKind::Control && p.rect.contains(x, y) {
                if self.volume_slider().grow(8).contains(x, y) {
                    self.drag = Some(Drag::Volume);
                    self.set_volume_at(l, x);
                } else if self.control_settings_btn().contains(x, y) {
                    self.close_popup(l);
                    self.open_id(l, "settings");
                }
                return;
            }
            let choice = p.item_rects().into_iter().find(|(r, _)| r.contains(x, y)).and_then(|(_, i)| p.items[i].1);
            self.close_popup(l);
            if let Some(act) = choice {
                self.menu_act(l, act);
            }
            return;
        }
        if let Some(la) = &self.launcher {
            if la.closing {
                return;
            }
            let visible: Vec<Launch> = la.visible().iter().map(|e| e.launch.clone()).collect();
            let tiles = self.launcher_tiles(visible.len(), la.scroll);
            if let Some(i) = tiles.iter().position(|t| t.contains(x, y) && y > 130) {
                let what = visible[i].clone();
                self.close_launcher(l, Some(what));
            } else if !self.search_rect().contains(x, y) {
                self.close_launcher(l, None);
            }
            return;
        }
        let bar = !self.full_screen();
        if bar && self.logo_btn().contains(x, y) {
            let b = self.logo_btn();
            let p = Popup::menu(
                b.x,
                b.bottom() + 12,
                alloc::vec![
                    ("Info su arcOS", Some(MenuAct::About)),
                    ("", None),
                    ("Impostazioni di sistema…", Some(MenuAct::Settings)),
                    ("Negozio…", Some(MenuAct::Store)),
                    ("Monitor attività", Some(MenuAct::Activity)),
                    ("", None),
                    ("Riavvia…", Some(MenuAct::Restart)),
                    ("Spegni…", Some(MenuAct::PowerOff)),
                ],
                self.w,
            );
            self.show_popup(l, p);
            return;
        }
        if bar && self.power_btn().contains(x, y) {
            let t = self.topbar();
            let p = Popup::menu(
                t.right() - 230,
                t.bottom() + 8,
                alloc::vec![("Riavvia", Some(MenuAct::Restart)), ("Spegni", Some(MenuAct::PowerOff))],
                self.w,
            );
            self.show_popup(l, p);
            return;
        }
        if bar && self.control_btn().contains(x, y) {
            let p = Popup { kind: PopupKind::Control, rect: self.control_rect(), items: Vec::new(), anim: Anim::new(160) };
            self.show_popup(l, p);
            return;
        }
        let items = self.dock_items();
        let (rects, bg) = self.dock_layout(&items);
        if self.dock_shown() && (bg.contains(x, y) || rects.iter().any(|r| r.contains(x, y))) {
            if let Some(i) = rects.iter().position(|r| r.grow(DOCK_GAP / 2).contains(x, y)) {
                self.dock_press(l, &items[i]);
            }
            return;
        }
        // Windows, the top one first.
        let hit = self
            .windows
            .iter()
            .rev()
            .filter(|w| !w.minimized && !self.moving(w.id))
            .find_map(|w| w.hit(x, y).map(|h| (w.id, h)));
        if let Some((id, hit)) = hit {
            if self.focused() != Some(id) {
                self.raise(l, id);
            }
            match hit {
                Hit::Close => self.close_window(l, id),
                Hit::Minimize => self.minimize(l, id),
                Hit::Maximize => self.zoom(l, id),
                Hit::Title => {
                    if double {
                        self.zoom(l, id);
                    } else if let Some(win) = self.window(id) {
                        self.drag = Some(Drag::Move(id, x - win.rect.x, y - win.rect.y));
                    }
                }
                Hit::Resize => {
                    if let Some(win) = self.window(id) {
                        self.drag = Some(Drag::Resize(id, win.rect.right() - x, win.rect.bottom() - y));
                    }
                }
                Hit::Body(bx, by) => {
                    let Some(win) = self.window_mut(id) else { return };
                    let body = win.body();
                    let action = win.app.press(bx, by, double, body);
                    self.drag = Some(Drag::Body(id));
                    self.act(l, Some(id), action);
                }
            }
        }
    }

    fn dock_press(&mut self, l: &mut Looper, item: &DockItem) {
        if item.id == "launcher" {
            self.toggle_launcher(l);
            return;
        }
        // An open window of this kind: show it (the top one).
        let kind = item.kind.as_str();
        let top = self.windows.iter().rev().find(|w| w.app.kind() == kind).map(|w| (w.id, w.minimized));
        match top {
            Some((id, minimized)) => {
                if !minimized && self.focused() == Some(id) && item.id != "trash" {
                    // Already in front: bring up any minimised sibling.
                    if let Some(other) = self.windows.iter().find(|w| w.app.kind() == kind && w.minimized).map(|w| w.id) {
                        self.raise(l, other);
                    }
                } else {
                    self.raise(l, id);
                }
            }
            None => {
                if let Some(launch) = item.launch.clone() {
                    self.launch(l, launch);
                }
            }
        }
    }

    fn key(&mut self, l: &mut Looper, k: KeyEvent) {
        if self.ending.is_some() {
            return;
        }
        if k.ctrl && k.key == Key::Char(' ') {
            self.toggle_launcher(l);
            return;
        }
        if self.popup.is_some() && k.key == Key::Escape {
            self.close_popup(l);
            return;
        }
        if let Some(la) = &mut self.launcher {
            if la.closing {
                return;
            }
            match k.key {
                Key::Escape => self.close_launcher(l, None),
                Key::Enter => {
                    let first = la.visible().first().map(|e| e.launch.clone());
                    if first.is_some() {
                        self.close_launcher(l, first);
                    }
                }
                _ => {
                    if ui::edit_text(&mut la.query, &k, 40) {
                        la.scroll = 0;
                    }
                    l.damage(self.screen());
                }
            }
            return;
        }
        // Alt+Tab: the next window comes to the front.
        if k.alt && k.key == Key::Tab {
            if let Some(id) = self.windows.iter().find(|w| !self.moving(w.id)).map(|w| w.id) {
                if self.windows.len() > 1 {
                    self.raise(l, id);
                }
            }
            return;
        }
        if let Some(id) = self.focused() {
            if k.ctrl && matches!(k.key, Key::Char('q') | Key::Char('Q')) {
                self.close_window(l, id);
                return;
            }
            // F11: full screen and back.
            if k.key == Key::F(11) {
                self.zoom(l, id);
                return;
            }
            if let Some(win) = self.window_mut(id) {
                let action = win.app.key(k);
                self.act(l, Some(id), action);
            }
        }
    }

    fn moved(&mut self, l: &mut Looper, x: i32, y: i32, held: bool) {
        self.mouse = (x, y);
        if held {
            match self.drag {
                Some(Drag::Move(id, dx, dy)) => {
                    let area = self.screen();
                    let top = self.topbar().bottom();
                    let mut new_drag = None;
                    if let Some(win) = self.window_mut(id) {
                        let before = win.bounds();
                        if win.restore.take().is_some() {
                            // Dragging a maximised window: back to its size,
                            // under the pointer.
                            let (w, h) = (win.rect.w, win.rect.h);
                            let (ow, oh) = (win.app.size().0.min(w), win.app.size().1 + TITLE_H + GAP);
                            win.rect = Rect::new(x - ow / 2, y - 20, ow, oh.min(h));
                            new_drag = Some(Drag::Move(id, ow / 2, 20));
                        } else {
                            win.rect.x = (x - dx).clamp(area.x - win.rect.w + 80, area.right() - 80);
                            win.rect.y = (y - dy).clamp(top - 10, area.bottom() - 60);
                        }
                        let after = win.bounds();
                        l.damage(before.union(&after));
                    }
                    if new_drag.is_some() {
                        self.drag = new_drag;
                    }
                    return;
                }
                Some(Drag::Resize(id, dx, dy)) => {
                    let area = self.work_area();
                    if let Some(win) = self.window_mut(id) {
                        let before = win.bounds();
                        let (mw, mh) = win.min_size();
                        win.rect.w = (x + dx - win.rect.x).clamp(mw, area.right() - win.rect.x);
                        win.rect.h = (y + dy - win.rect.y).clamp(mh, (area.bottom() - win.rect.y).max(mh));
                        let after = win.bounds();
                        l.damage(before.union(&after));
                    }
                    return;
                }
                Some(Drag::Body(id)) => {
                    if let Some(win) = self.window_mut(id) {
                        let body = win.body();
                        if win.app.drag(x - body.x, y - body.y, body) {
                            l.damage(win.bounds());
                        }
                    }
                    return;
                }
                Some(Drag::Volume) => {
                    self.set_volume_at(l, x);
                    return;
                }
                None => {}
            }
        }
        if let Some(p) = &self.popup {
            l.damage(p.rect);
        }
        if let Some(la) = &self.launcher {
            if !la.closing {
                l.damage(Rect::new(0, 130, self.w, self.h - 130));
            }
            return;
        }
        // Full screen: the title pill comes down at the top edge, the dock up
        // at the bottom one.
        if self.full_screen() {
            let dock_zone = self.dock_zone();
            let dock_reveal = y >= self.h - 3 || (self.dock_reveal && dock_zone.contains(x, y));
            if dock_reveal != self.dock_reveal {
                self.dock_reveal = dock_reveal;
                l.damage(dock_zone);
            }
            if let Some(id) = self.focused() {
                if let Some(win) = self.window_mut(id) {
                    let pill = win.title_rect();
                    let reveal = y < 6 || (win.reveal && pill.grow(16).contains(x, y));
                    if reveal != win.reveal {
                        win.reveal = reveal;
                        win.hover = None;
                        l.damage(win.bounds());
                    }
                }
            }
        } else if self.dock_reveal {
            self.dock_reveal = false;
        }
        // The dock magnifies around the pointer.
        let zone = self.dock();
        let near = Rect::new(zone.x - 20, zone.y - MAGNIFY - 10, zone.w + 40, zone.h + MAGNIFY + 20);
        let inside = near.contains(x, y) && self.popup.is_none() && self.dock_shown();
        self.mag.target = if inside { 1000 } else { 0 };
        let items = self.dock_items();
        let hover = if inside {
            let (rects, _) = self.dock_layout(&items);
            rects.iter().position(|r| r.grow(DOCK_GAP / 2).contains(x, y) || (x >= r.x - DOCK_GAP / 2 && x < r.right() + DOCK_GAP / 2 && y > r.y - 4))
        } else {
            None
        };
        if inside || self.mag.moving() || hover != self.hover {
            l.damage(self.dock_zone());
        }
        self.hover = hover;
        // Window buttons light up; the app under the pointer hears of it.
        let under = self
            .windows
            .iter()
            .rev()
            .filter(|w| !w.minimized && !self.moving(w.id))
            .find(|w| w.rect.contains(x, y))
            .map(|w| w.id);
        if self.hovered != under {
            if let Some(old) = self.hovered.and_then(|id| self.windows.iter_mut().find(|w| w.id == id)) {
                let body = old.body();
                let redraw = old.app.hover(None, body) | old.hover.take().is_some();
                if redraw {
                    l.damage(old.bounds());
                }
            }
            self.hovered = under;
        }
        if let Some(win) = under.and_then(|id| self.windows.iter_mut().find(|w| w.id == id)) {
            let btn = win.buttons().iter().position(|b| b.contains(x, y));
            if btn != win.hover {
                win.hover = btn;
                l.damage(ui::with_shadow(win.title_rect()));
            }
            let body = win.body();
            let at = if body.contains(x, y) { Some((x - body.x, y - body.y)) } else { None };
            if win.app.hover(at, body) {
                l.damage(body);
            }
        }
    }

    fn released(&mut self, l: &mut Looper) {
        if let Some(Drag::Body(id)) = self.drag {
            if let Some(win) = self.window_mut(id) {
                let body = win.body();
                let action = win.app.release(body);
                self.act(l, Some(id), action);
            }
        }
        self.drag = None;
    }

    // ------------------------------------------------------------------
    // Drawing

    fn draw_topbar(&self, c: &mut Canvas) {
        let t = self.topbar();
        ui::glass(c, &self.blur, t, 16, ui::GLASS);
        let lb = self.logo_btn();
        if lb.contains(self.mouse.0, self.mouse.1) || self.popup.as_ref().is_some_and(|p| p.rect.x < lb.right() && p.kind == PopupKind::Menu) {
            c.fill_round(lb, 10, 0x26FF_FFFF);
        }
        draw_logo(c, lb.x + lb.w / 2, lb.y + lb.h / 2, 14, 0xFFFF_FFFF);
        // The app in front, as macOS shows it.
        let app = self
            .focused()
            .and_then(|id| self.window(id))
            .map(|w| apps::app_name(w.app.kind(), &w.app.title()))
            .unwrap_or_else(|| String::from("Scrivania"));
        text::draw(c, Font::Strong, lb.right() + 8, t.y + 11, &text::fit(Font::Strong, &app, 200), TEXT);
        let d = now();
        let clock = format!("{:02}:{:02}", d.hour, d.min);
        let date = format!("{} {} {}", WEEKDAYS_SHORT[d.weekday as usize % 7], d.day, MONTHS_SHORT[(d.month as usize + 11) % 12]);
        let cw = text::width(Font::Strong, &clock) + 10 + text::width(Font::Body, &date);
        let x = t.x + (t.w - cw) / 2;
        let w = text::draw(c, Font::Strong, x, t.y + 11, &clock, TEXT);
        text::draw(c, Font::Body, x + w + 10, t.y + 11, &date, TEXT_DIM);
        // Status: the control centre (volume, network), then the power button.
        let cb = self.control_btn();
        if cb.contains(self.mouse.0, self.mouse.1) || self.popup.as_ref().is_some_and(|p| p.kind == PopupKind::Control) {
            c.fill_round(cb, 12, 0x26FF_FFFF);
        }
        let vol = if crate::dev::audio::volume() == 0 { "g-mute" } else { "g-volume" };
        res::draw_icon(c, vol, 18, cb.x + 8, cb.y + 5);
        if crate::net::online() {
            res::draw_icon(c, "g-wifi", 18, cb.x + 36, cb.y + 5);
        } else {
            res::draw_icon_faded(c, "g-wifi", 18, cb.x + 36, cb.y + 5, 90);
        }
        let p = self.power_btn();
        c.fill_circle(p.x + 14, p.y + 14, 14, if p.contains(self.mouse.0, self.mouse.1) { 0x40FF_FFFF } else { 0x26FF_FFFF });
        res::draw_icon(c, "g-power", 16, p.x + 6, p.y + 6);
    }

    fn draw_widgets(&self, c: &mut Canvas, r: Rect) {
        let n = self.show_widgets();
        let ws = self.widgets();
        let d = now();
        let name = settings::get().name;
        if n > 0 && r.overlaps(&ui::with_shadow(ws[0])) {
            let g = ws[0];
            ui::card(c, &self.blur, g, 22, ui::GLASS);
            let hello = match d.hour {
                5..=11 => "Buongiorno",
                12..=17 => "Buon pomeriggio",
                _ => "Buonasera",
            };
            let line = if name.is_empty() { String::from(hello) } else { format!("{hello}, {name}") };
            let date = format!("{} {} {}", WEEKDAYS[d.weekday as usize % 7], d.day, MONTHS[(d.month as usize + 11) % 12]);
            ui::caps(c, g.x + 22, g.y + 20, &date, TEXT_FAINT);
            text::draw(c, Font::Clock, g.x + 18, g.y + 36, &format!("{:02}:{:02}", d.hour, d.min), TEXT);
            text::draw(c, Font::Heading, g.x + 22, g.y + 112, &text::fit(Font::Heading, &line, g.w - 40), TEXT);
        }
        if n > 1 && r.overlaps(&ui::with_shadow(ws[1])) {
            let g = ws[1];
            ui::card(c, &self.blur, g, 22, ui::GLASS);
            let title = format!("{} {}", capitalize(MONTHS[(d.month as usize + 11) % 12]), d.year);
            text::draw(c, Font::Heading, g.x + 20, g.y + 14, &title, TEXT);
            let cell = (g.w - 30) / 7;
            for (i, h) in ["L", "M", "M", "G", "V", "S", "D"].iter().enumerate() {
                let w = ui::caps_width(h);
                ui::caps(c, g.x + 15 + i as i32 * cell + (cell - w) / 2, g.y + 48, h, TEXT_FAINT);
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
                    text::draw_centered(c, Font::Strong, cx, cy - 9, &label, 0xFF0E_0B0C);
                } else {
                    text::draw_centered(c, Font::Body, cx, cy - 9, &label, if col >= 5 { TEXT_DIM } else { TEXT });
                }
            }
        }
        if n > 2 && r.overlaps(&ui::with_shadow(ws[2])) {
            let g = ws[2];
            ui::card(c, &self.blur, g, 22, ui::GLASS);
            ui::caps(c, g.x + 20, g.y + 20, "Sistema", TEXT_FAINT);
            let (free, total) = crate::mem::frame::stats();
            let used = total.saturating_sub(free);
            let bar = Rect::new(g.x + 20, g.y + 50, g.w - 40, 8);
            c.fill_round(bar, 4, 0x26FF_FFFF);
            let fw = (bar.w as u64 * used).checked_div(total).unwrap_or(0) as i32;
            c.fill_round(Rect::new(bar.x, bar.y, fw.max(8), bar.h), 4, ui::ACCENT);
            text::draw(c, Font::Small, g.x + 20, g.y + 66, &format!("Memoria: {} in uso su {}", Size(used), Size(total)), TEXT_DIM);
            let up = time::uptime_ms() / 60_000;
            let net = if crate::net::online() { "connessa" } else { "assente" };
            text::draw(c, Font::Small, g.x + 20, g.y + 88, &format!("Acceso da {} h {} min · rete {net}", up / 60, up % 60), TEXT_DIM);
        }
    }

    fn draw_dock(&self, c: &mut Canvas) {
        let items = self.dock_items();
        let (rects, bg) = self.dock_layout(&items);
        ui::glass(c, &self.blur, bg, 24, ui::GLASS);
        for (i, it) in items.iter().enumerate() {
            let r = rects[i];
            if it.split && i > 0 {
                let x = r.x - (SEP_W + DOCK_GAP) / 2 - 1;
                c.fill(Rect::new(x, bg.y + 16, 1, bg.h - 32), 0x40FF_FFFF);
            }
            let mut lift = 0;
            if let Some((what, a)) = &self.bounce {
                if *what == it.kind || (what.starts_with('@') && it.icon == *what) {
                    // Two hops, the second lower.
                    let t = a.t() as i64;
                    let (phase, height) = if t < 600 { (t * 1000 / 600, 22) } else { ((t - 600) * 1000 / 400, 10) };
                    lift = (height * 4 * phase * (1000 - phase) / 1_000_000) as i32;
                }
            }
            res::draw_icon(c, &it.icon, r.w, r.x, r.y - lift);
            let open = self.windows.iter().any(|w| w.app.kind() == it.kind) && it.id != "trash";
            if open {
                c.fill_circle(r.x + r.w / 2, bg.bottom() - 6, 2, 0xE6FF_FFFF);
            }
        }
        if let Some(i) = self.hover.filter(|&i| i < items.len()) {
            let r = rects[i];
            let label = &items[i].label;
            let w = text::width(Font::Strong, label) + 24;
            let tip = Rect::new(r.x + r.w / 2 - w / 2, r.y - 44, w, 30);
            ui::glass(c, &self.blur, tip, 15, ui::GLASS_STRONG);
            text::draw_centered(c, Font::Strong, tip.x + w / 2, tip.y + 6, label, TEXT);
        }
    }

    fn draw_launcher(&self, c: &mut Canvas, la: &Launcher) {
        let t = la.shown();
        let full = t >= 1000;
        let screen = self.screen();
        if full {
            c.blit(&self.blur, 0, 0, screen);
        } else {
            let cl = c.clip();
            c.blit_faded(&self.blur, cl.x, cl.y, cl, t * 255 / 1000);
        }
        c.fill(c.clip(), ((0x8C * t / 1000) << 24) | 0x10_0A16);
        let alpha = |col: u32| (((col >> 24) * t / 1000) << 24) | (col & 0xFF_FFFF);
        let s = self.search_rect();
        let s = Rect::new(s.x, s.y - (1000 - t as i32) * 20 / 1000, s.w, s.h);
        c.fill_round(s, 20, alpha(0x33FF_FFFF));
        c.stroke_round(s, 20, 1, alpha(ui::BORDER));
        res::draw_icon_faded(c, "g-search", 16, s.x + 16, s.y + 12, t * 255 / 1000);
        let (lh, _) = text::metrics(Font::Body);
        if la.query.is_empty() {
            text::draw(c, Font::Body, s.x + 42, s.y + (s.h - lh) / 2, "Cerca", alpha(TEXT_FAINT));
        } else {
            let w = text::draw(c, Font::Body, s.x + 42, s.y + (s.h - lh) / 2, &la.query, alpha(TEXT));
            c.fill(Rect::new(s.x + 44 + w, s.y + 10, 2, s.h - 20), alpha(0xFFFF_FFFF));
        }
        let visible = la.visible();
        if visible.is_empty() {
            text::draw_centered(c, Font::Body, self.w / 2, 190, "Nessuna app con questo nome", alpha(TEXT_DIM));
        }
        // Tiles zoom in from a little larger, as in Launchpad.
        let zoom = ui::lerp(1120, 1000, t);
        let cx = self.w / 2;
        let saved = c.clip();
        c.set_clip(saved.intersect(&Rect::new(0, 128, self.w, self.h - 128)));
        for (e, tile) in visible.iter().zip(self.launcher_tiles(visible.len(), la.scroll)) {
            let tile = Rect::new(cx + (tile.x - cx) * zoom / 1000, 150 + (tile.y - 150) * zoom / 1000, tile.w, tile.h);
            if tile.bottom() < 128 || tile.y > self.h {
                continue;
            }
            if full && tile.contains(self.mouse.0, self.mouse.1) {
                c.fill_round(tile.grow(-6), 18, 0x22FF_FFFF);
            }
            let size = (64 * zoom / 1000) & !1;
            res::draw_icon_faded(c, &e.icon, size, tile.x + (tile.w - size) / 2, tile.y + 12, t * 255 / 1000);
            let label = text::fit(Font::Body, &e.name, tile.w - 12);
            text::draw_centered(c, Font::Body, tile.x + tile.w / 2, tile.y + 86, &label, alpha(TEXT));
        }
        c.set_clip(saved);
        text::draw_centered(c, Font::Small, self.w / 2, self.h - 36, "Ctrl+Spazio apre e chiude · Esc per uscire", alpha(TEXT_FAINT));
    }

    fn draw_popup(&self, c: &mut Canvas, p: &Popup) {
        let saved = c.clip();
        c.set_clip(saved.intersect(&p.shown()));
        ui::glass(c, &self.blur, p.rect, 14, ui::GLASS_STRONG);
        match p.kind {
            PopupKind::Menu => {
                let mut y = p.rect.y + 6;
                for (label, act) in &p.items {
                    if act.is_none() {
                        c.fill(Rect::new(p.rect.x + 14, y + 5, p.rect.w - 28, 1), ui::SEPARATOR);
                        y += 11;
                        continue;
                    }
                    let r = Rect::new(p.rect.x + 6, y, p.rect.w - 12, 32);
                    if r.contains(self.mouse.0, self.mouse.1) {
                        c.fill_round(r, 8, ui::SELECTED);
                    }
                    text::draw(c, Font::Body, r.x + 12, r.y + 7, label, TEXT);
                    y += 34;
                }
            }
            PopupKind::Control => {
                let r = p.rect;
                text::draw(c, Font::Strong, r.x + 18, r.y + 16, "Suono", TEXT);
                let vol = crate::dev::audio::volume();
                let s = self.volume_slider();
                res::draw_icon(c, if vol == 0 { "g-mute" } else { "g-volume" }, 20, r.x + 20, s.y + 2);
                ui::slider(c, s, vol * 10);
                let card = Rect::new(r.x + 16, r.y + 96, r.w - 32, 68);
                c.fill_round(card, 14, ui::PANEL);
                let online = crate::net::online();
                c.fill_circle(card.x + 30, card.y + 34, 18, if online { ui::ACCENT } else { 0x40FF_FFFF });
                res::draw_icon(c, "g-wifi", 18, card.x + 21, card.y + 25);
                text::draw(c, Font::Strong, card.x + 60, card.y + 14, "Rete", TEXT);
                let line = match crate::net::address() {
                    Some(a) if online => format!("Connessa · {a}"),
                    _ => String::from("Nessuna connessione"),
                };
                text::draw(c, Font::Small, card.x + 60, card.y + 38, &text::fit(Font::Small, &line, card.w - 72), TEXT_DIM);
                let b = self.control_settings_btn();
                ui::button(c, b, "Impostazioni…", false, b.contains(self.mouse.0, self.mouse.1));
            }
        }
        c.set_clip(saved);
    }

    fn draw_notes(&self, c: &mut Canvas) {
        for (i, n) in self.notes.iter().enumerate() {
            let r = self.note_rect(i, n.slide());
            if r.x >= self.w {
                continue;
            }
            ui::glass(c, &self.blur, r, 18, ui::GLASS_STRONG);
            res::draw_icon(c, &n.icon, 40, r.x + 14, r.y + 18);
            text::draw(c, Font::Strong, r.x + 66, r.y + 14, &text::fit(Font::Strong, &n.title, r.w - 84), TEXT);
            let lines = text::wrap(Font::Small, &n.body, r.w - 84);
            for (k, line) in lines.iter().take(2).enumerate() {
                text::draw(c, Font::Small, r.x + 66, r.y + 36 + k as i32 * 17, line, TEXT_DIM);
            }
        }
    }

    fn draw_ending(&self, c: &mut Canvas, a: &Anim) {
        let t = a.out();
        c.fill(c.clip(), (t * 255 / 1000) << 24);
        if t > 300 {
            let o = (t - 300) * 255 / 700;
            draw_logo(c, self.w / 2, self.h / 2 - 20, 48, (o << 24) | 0xFF_FFFF);
        }
    }
}

fn capitalize(s: &str) -> String {
    let mut ch = s.chars();
    ch.next().map(|f| f.to_uppercase().chain(ch).collect()).unwrap_or_default()
}

use crate::fs::Path;

impl View for Desktop {
    fn resized(&mut self, w: i32, h: i32) {
        self.w = w;
        self.h = h;
        let (sharp, blur) = res::wallpaper(&settings::get().wallpaper, w, h);
        self.sharp = sharp;
        self.blur = blur;
        self.popup = None;
        self.fx.clear();
        let area = self.work_area();
        let full = self.full_area();
        for win in self.windows.iter_mut() {
            if win.restore.is_some() {
                win.rect = full; // maximised: fill the new screen
            } else {
                win.rect.w = win.rect.w.min(area.w);
                win.rect.h = win.rect.h.min(area.h);
                win.rect.x = win.rect.x.clamp(area.x, (area.right() - win.rect.w).max(area.x));
                win.rect.y = win.rect.y.clamp(area.y, (area.bottom() - win.rect.h).max(area.y));
            }
        }
    }

    fn animating(&self) -> bool {
        !self.fx.is_empty()
            || self.launcher.as_ref().is_some_and(|l| !l.anim.done() || l.closing)
            || self.popup.as_ref().is_some_and(|p| !p.anim.done())
            || self.mag.moving()
            || self.bounce.is_some()
            || self.notes.iter().any(|n| n.moving())
            || self.ending.is_some()
            || self.windows.iter().any(|w| !w.minimized && w.app.animating())
    }

    fn event(&mut self, l: &mut Looper, ev: Event) -> bool {
        let quit = self.handle(l, ev);
        // Into or out of full screen: the bar and the dock come and go.
        let full = self.full_screen();
        if full != self.was_full {
            self.was_full = full;
            self.dock_reveal = false;
            l.damage(self.screen());
        }
        quit
    }

    fn draw(&mut self, c: &mut Canvas, r: Rect) {
        self.paint(c, r);
    }
}

impl Desktop {
    fn handle(&mut self, l: &mut Looper, ev: Event) -> bool {
        match ev {
            Event::Press { x, y, double } => self.press(l, x, y, double),
            Event::Menu { x, y } => {
                if self.launcher.is_some() || self.ending.is_some() {
                    return false;
                }
                let on_window =
                    self.windows.iter().rev().filter(|w| !w.minimized && !self.moving(w.id)).find(|w| w.rect.contains(x, y)).map(|w| w.id);
                if let Some(id) = on_window {
                    if let Some(win) = self.window_mut(id) {
                        let b = win.body();
                        if b.contains(x, y) {
                            let action = win.app.menu(x - b.x, y - b.y, b);
                            self.act(l, Some(id), action);
                        }
                    }
                    return false;
                }
                if self.dock().contains(x, y) {
                    return false;
                }
                let p = Popup::menu(
                    x,
                    y,
                    alloc::vec![
                        ("Cambia sfondo", Some(MenuAct::NextWallpaper)),
                        ("Impostazioni Schermo…", Some(MenuAct::Display)),
                        ("", None),
                        ("Info su arcOS", Some(MenuAct::About)),
                    ],
                    self.w,
                );
                self.show_popup(l, p);
            }
            Event::Release => self.released(l),
            Event::Move { x, y, held } => self.moved(l, x, y, held),
            Event::Wheel { lines } => {
                let (x, y) = self.mouse;
                if let Some(la) = &mut self.launcher {
                    let n = la.visible().len() as i32;
                    let cols = 7.min((self.w - 80) / 128).max(1);
                    let rows = (n + cols - 1) / cols;
                    let max = (rows * 120 + 170 - (self.h - 70)).max(0);
                    la.scroll = (la.scroll + lines * 60).clamp(0, max);
                    l.damage(self.screen());
                    return false;
                }
                if let Some(win) = self.windows.iter_mut().rev().find(|w| !w.minimized && w.rect.contains(x, y)) {
                    if win.body().contains(x, y) && win.app.scroll(lines) {
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
                // Programs that ended take their windows with them.
                let gone: Vec<u32> = self.windows.iter().filter(|w| !w.app.alive()).map(|w| w.id).collect();
                for id in gone {
                    self.close_window(l, id);
                }
                // New windows of programs, and notifications from the system.
                for app in super::window::take_new() {
                    self.open(l, app, false);
                }
                while let Some((title, body, icon)) = super::take_notification() {
                    self.notify(l, title, body, icon);
                }
                if self.notes.iter().any(|n| n.gone()) {
                    self.notes.retain(|n| !n.gone());
                    l.damage(self.notes_zone());
                }
            }
            Event::Frame => {
                // Windows in motion.
                let mut finished = Vec::new();
                for (i, f) in self.fx.iter_mut().enumerate() {
                    let (r, _) = f.place();
                    let r = r.grow(2);
                    l.damage(f.last.union(&r));
                    f.last = r;
                    if f.anim.done() {
                        finished.push(i);
                    }
                }
                for &i in finished.iter().rev() {
                    let f = self.fx.remove(i);
                    l.damage(f.last.union(&f.at));
                    if f.kind == FxKind::Minimize {
                        if let Some(w) = self.window_mut(f.id) {
                            w.minimized = true;
                        }
                        l.damage(self.dock_zone());
                    }
                }
                if let Some(la) = &self.launcher {
                    if !la.anim.done() || la.closing {
                        l.damage(self.screen());
                    }
                    if la.closing && la.anim.done() {
                        let then = self.launcher.take().and_then(|la| la.then);
                        if let Some(what) = then {
                            self.launch(l, what);
                        }
                    }
                }
                if let Some(p) = &self.popup {
                    if !p.anim.done() {
                        l.damage(ui::with_shadow(p.rect));
                    }
                }
                if self.mag.step() {
                    l.damage(self.dock_zone());
                }
                if let Some((_, a)) = &self.bounce {
                    l.damage(self.dock_zone());
                    if a.done() {
                        self.bounce = None;
                    }
                }
                if self.notes.iter().any(|n| n.moving()) {
                    l.damage(self.notes_zone());
                }
                for w in self.windows.iter_mut() {
                    if !w.minimized && w.app.animating() && w.app.frame() {
                        l.damage(w.body());
                    }
                }
                if let Some((how, a)) = &self.ending {
                    l.damage(self.screen());
                    if a.elapsed() > 900 {
                        match how {
                            Ending::Restart => crate::acpi::reboot(),
                            Ending::PowerOff => crate::acpi::power_off(),
                        }
                    }
                }
            }
        }
        false
    }

    fn paint(&mut self, c: &mut Canvas, r: Rect) {
        if let Some(la) = &self.launcher {
            if la.shown() >= 1000 {
                self.draw_launcher(c, la);
                return;
            }
        }
        c.blit(&self.sharp, r.x, r.y, r);
        self.draw_widgets(c, r);
        let focused = self.focused();
        let blur = &self.blur;
        let fx = &self.fx;
        for w in self.windows.iter_mut() {
            if !w.minimized && r.overlaps(&w.bounds()) && !fx.iter().any(|f| f.id == w.id) {
                let f = Some(w.id) == focused;
                w.draw(c, blur, f, r);
            }
        }
        for f in &self.fx {
            let (dst, opacity) = f.place();
            if r.overlaps(&dst) {
                c.draw_scaled(&f.snap, dst, opacity);
            }
        }
        if self.dock_shown() && r.overlaps(&self.dock_zone()) {
            self.draw_dock(c);
        }
        if !self.full_screen() && r.overlaps(&ui::with_shadow(self.topbar())) {
            self.draw_topbar(c);
        }
        if let Some(p) = &self.popup {
            if r.overlaps(&ui::with_shadow(p.rect)) {
                self.draw_popup(c, p);
            }
        }
        if !self.notes.is_empty() && r.overlaps(&self.notes_zone()) {
            self.draw_notes(c);
        }
        if let Some(la) = &self.launcher {
            self.draw_launcher(c, la);
        }
        if let Some((_, a)) = &self.ending {
            self.draw_ending(c, a);
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
        popup: None,
        fx: Vec::new(),
        notes: Vec::new(),
        hover: None,
        mag: Glide::new(0),
        bounce: None,
        mouse: (-1, -1),
        minute: u64::MAX,
        second: 0,
        cascade: 0,
        hovered: None,
        ending: None,
        was_full: false,
        dock_reveal: false,
    };
    let mut l = Looper::new(d);
    l.damage_all(d);
    startup_sound();
    l.run(d, &mut desk);
}

/// The chime when the desktop appears, if there is a sound card and the
/// user wants it.
fn startup_sound() {
    if !settings::get().startup_sound || !crate::dev::audio::available() {
        return;
    }
    let file = fs::abs("sys:/system/sounds/avvio.wav").and_then(|p| fs::read_file(&p));
    match file.map(|b| crate::dev::audio::parse_wav(&b)) {
        Ok(Ok(sound)) => {
            let _ = crate::dev::audio::play(sound);
        }
        Ok(Err(e)) => crate::warn!("gui", "startup sound: {e}"),
        Err(e) => crate::warn!("gui", "startup sound: {e}"),
    }
}
