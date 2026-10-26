//! ticly Do: tasks, notes and the calendar in one place, in the look of
//! do.ticly.it. The data are the system's own (Promemoria, Note,
//! Calendario), so what is written here shows up in those apps and the
//! other way round; the ticly account names the user and holds the profile.

use super::ticly::{self as tui, Login};
use crate::dev::input::{Key, KeyEvent};
use crate::fs;
use crate::gui::gfx::{Canvas, Rect};
use crate::gui::text::{self, Font};
use crate::gui::ui::{self, TEXT, TEXT_DIM, TEXT_FAINT};
use crate::gui::wm::{Action, App};
use crate::ticly::{self, Account};
use crate::time;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

const SIDEBAR: i32 = 280;
const TASKS: &str = "sys:/home/Promemoria.txt";
const NOTES: &str = "sys:/home/Note";
const EVENTS: &str = "sys:/home/Calendario.txt";
const SECTIONS: [(&str, &str); 5] = [("Panoramica", "g-grid"), ("Task", "reminders"), ("Note", "notes"), ("Calendario", "calendar"), ("Profilo", "info")];
const WEEKDAYS: [&str; 7] = ["Lunedì", "Martedì", "Mercoledì", "Giovedì", "Venerdì", "Sabato", "Domenica"];

struct Note {
    title: String,
    preview: String,
    path: String,
    modified: u64,
}

struct Event {
    date: (u64, u64, u64),
    time: String,
    text: String,
}

fn read_text(path: &str) -> Option<String> {
    fs::abs(path).and_then(|p| fs::read_file(&p)).ok().map(|b| String::from_utf8_lossy(&b).into_owned())
}

fn load_tasks() -> Vec<(bool, String)> {
    read_text(TASKS)
        .unwrap_or_default()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| match l.strip_prefix("[x] ") {
            Some(t) => (true, t.to_string()),
            None => (false, l.to_string()),
        })
        .collect()
}

fn save_tasks(t: &[(bool, String)]) {
    let mut out = String::new();
    for (done, text) in t {
        if *done {
            out.push_str("[x] ");
        }
        out.push_str(text);
        out.push('\n');
    }
    if let Ok(p) = fs::abs(TASKS) {
        let _ = fs::write_file(&p, out.as_bytes());
    }
}

fn load_notes() -> Vec<Note> {
    let Ok(dir) = fs::abs(NOTES) else { return Vec::new() };
    let mut v: Vec<Note> = fs::list_dir(&dir)
        .unwrap_or_default()
        .into_iter()
        .filter(|(e, _)| e.name.ends_with(".txt"))
        .map(|(e, st)| {
            let path = format!("{NOTES}/{}", e.name);
            let body = read_text(&path).unwrap_or_default();
            let mut lines = body.lines().filter(|l| !l.trim().is_empty());
            let title = lines.next().unwrap_or("Nota senza titolo").to_string();
            let preview = lines.next().unwrap_or("").to_string();
            Note { title, preview, path, modified: st.modified }
        })
        .collect();
    v.sort_by(|a, b| b.modified.cmp(&a.modified));
    v
}

fn load_events() -> Vec<Event> {
    read_text(EVENTS)
        .unwrap_or_default()
        .lines()
        .filter_map(|l| {
            let mut it = l.splitn(3, '\t');
            let d: Vec<u64> = it.next()?.split('-').filter_map(|x| x.parse().ok()).collect();
            if d.len() != 3 {
                return None;
            }
            Some(Event { date: (d[0], d[1], d[2]), time: it.next().unwrap_or("").to_string(), text: it.next().unwrap_or("").to_string() })
        })
        .collect()
}

fn save_events(ev: &[Event]) {
    let mut out = String::new();
    for e in ev {
        out.push_str(&format!("{:04}-{:02}-{:02}\t{}\t{}\n", e.date.0, e.date.1, e.date.2, e.time, e.text));
    }
    if let Ok(p) = fs::abs(EVENTS) {
        let _ = fs::write_file(&p, out.as_bytes());
    }
}

fn today() -> (u64, u64, u64) {
    let d = time::from_epoch(time::now());
    (d.year, d.month, d.day)
}

fn weekday(y: u64, m: u64, d: u64) -> usize {
    time::from_epoch(time::to_epoch(y, m, d, 12, 0, 0)).weekday as usize % 7
}

pub struct TiclyDo {
    account: Option<Account>,
    login: Option<Login>,
    section: usize,
    tasks: Vec<(bool, String)>,
    notes: Vec<Note>,
    events: Vec<Event>,
    /// The month shown and the selected day.
    month: (u64, u64),
    day: (u64, u64, u64),
    /// 0 month, 1 week, 2 day.
    view: usize,
    /// Text being typed: a new task or a new event.
    entry: String,
    typing: bool,
    hover: Option<Rect>,
}

impl TiclyDo {
    pub fn new() -> TiclyDo {
        let t = today();
        TiclyDo {
            account: ticly::account(),
            login: None,
            section: 0,
            tasks: load_tasks(),
            notes: load_notes(),
            events: load_events(),
            month: (t.0, t.1),
            day: t,
            view: 0,
            entry: String::new(),
            typing: false,
            hover: None,
        }
    }

    fn main(area: Rect) -> Rect {
        Rect::new(area.x + SIDEBAR + 10, area.y, area.w - SIDEBAR - 10, area.h)
    }

    fn nav(area: Rect, i: usize) -> Rect {
        Rect::new(area.x + 20, area.y + 92 + i as i32 * 52, SIDEBAR - 40, 46)
    }

    fn user_rect(area: Rect) -> Rect {
        Rect::new(area.x + 20, area.bottom() - 132, SIDEBAR - 40, 62)
    }

    fn exit_rect(area: Rect) -> Rect {
        Rect::new(area.x + 20, area.bottom() - 62, SIDEBAR - 40, 44)
    }

    fn events_on(&self, d: (u64, u64, u64)) -> Vec<&Event> {
        let mut v: Vec<&Event> = self.events.iter().filter(|e| e.date == d).collect();
        v.sort_by(|a, b| a.time.cmp(&b.time));
        v
    }

    fn draw_sidebar(&mut self, c: &mut Canvas, area: Rect) {
        tui::wordmark(c, area.x + 32, area.y + 34, "Do");
        for (i, (name, icon)) in SECTIONS.iter().enumerate() {
            let r = Self::nav(area, i);
            let sel = i == self.section;
            if sel {
                c.fill_round(r, 23, 0x40FF_FFFF);
                c.stroke_round(r, 23, 1, 0x33FF_FFFF);
            } else {
                c.fill_round(r, 16, if self.hover == Some(r) { ui::PANEL_HOVER } else { ui::PANEL });
            }
            crate::gui::res::draw_icon(c, icon, 18, r.x + 18, r.y + 14);
            let (lh, _) = text::metrics(Font::Strong);
            text::draw(c, if sel { Font::Strong } else { Font::Body }, r.x + 48, r.y + (r.h - lh) / 2, name, if sel { TEXT } else { TEXT_DIM });
        }
        let caption = if self.account.is_some() { "Account ticly" } else { "Su questo computer" };
        text::draw(c, Font::Small, area.x + 32, area.bottom() - 162, caption, TEXT_DIM);
        let ur = Self::user_rect(area);
        let ex = Self::exit_rect(area);
        match &self.account {
            Some(a) => {
                tui::user_card(c, ur, a);
                tui::sign_out_row(c, ex, "Esci Account", self.hover == Some(ex));
            }
            None => {
                c.fill_round(ur, 16, ui::PANEL);
                text::draw(c, Font::Small, ur.x + 16, ur.y + 12, "Accedi con il tuo account", TEXT_DIM);
                text::draw(c, Font::Small, ur.x + 16, ur.y + 32, "ticly per vedere il profilo.", TEXT_FAINT);
                ui::button(c, ex, "Accedi a ticly", true, self.hover == Some(ex));
            }
        }
    }

    fn stat_cards(m: Rect) -> [Rect; 3] {
        let w = (m.w - 112 - 24) / 3;
        let y = m.y + 160;
        [Rect::new(m.x + 56, y, w, 108), Rect::new(m.x + 56 + w + 12, y, w, 108), Rect::new(m.x + 56 + 2 * (w + 12), y, w, 108)]
    }

    fn draw_overview(&mut self, c: &mut Canvas, m: Rect) {
        let name = self.account.as_ref().map(|a| a.first_name()).unwrap_or_default();
        let hello = if name.is_empty() { String::from(tui::greeting()) } else { format!("{}, {name}", tui::greeting()) };
        text::draw(c, Font::Title, m.x + 56, m.y + 56, &hello, TEXT);
        let t = today();
        let date = format!("{} {} {}", WEEKDAYS[weekday(t.0, t.1, t.2)], t.2, capital(crate::gui::desktop::MONTHS[(t.1 as usize + 11) % 12]));
        text::draw(c, Font::Body, m.x + 56, m.y + 104, &date, TEXT_DIM);
        let open = self.tasks.iter().filter(|(d, _)| !d).count();
        let counts = [(open, "Task per oggi"), (self.events_on(t).len(), "Eventi oggi"), (self.notes.len(), "Note")];
        for (r, (n, label)) in Self::stat_cards(m).iter().zip(counts) {
            c.fill_round(*r, 20, ui::PANEL);
            c.stroke_round(*r, 20, 1, ui::BORDER);
            text::draw_centered(c, Font::Title, r.x + r.w / 2, r.y + 24, &format!("{n}"), TEXT);
            text::draw_centered(c, Font::Small, r.x + r.w / 2, r.y + 72, label, TEXT_DIM);
        }
        ui::caps(c, m.x + 56, m.y + 300, "Oggi", TEXT_FAINT);
        let mut y = m.y + 326;
        let evs: Vec<(String, String)> = self.events_on(t).iter().map(|e| (e.time.clone(), e.text.clone())).collect();
        let tasks: Vec<String> = self.tasks.iter().filter(|(d, _)| !d).map(|(_, s)| s.clone()).take(6).collect();
        if evs.is_empty() && tasks.is_empty() {
            text::draw(c, Font::Body, m.x + 60, y + 6, "Niente in programma. Goditi la giornata.", TEXT_DIM);
            return;
        }
        for (when, what) in evs {
            let r = Rect::new(m.x + 56, y, m.w - 112, 46);
            c.fill_round(r, 14, ui::PANEL);
            c.fill_round(Rect::new(r.x + 14, r.y + 12, 4, 22), 2, ui::ACCENT);
            text::draw(c, Font::Strong, r.x + 30, r.y + 14, &text::fit(Font::Strong, &what, r.w - 140), TEXT);
            text::draw_right(c, Font::Small, r.right() - 16, r.y + 16, if when.is_empty() { "Tutto il giorno" } else { &when }, TEXT_DIM);
            y += 54;
        }
        for t in tasks {
            if y > m.bottom() - 60 {
                break;
            }
            let r = Rect::new(m.x + 56, y, m.w - 112, 46);
            c.fill_round(r, 14, ui::PANEL);
            c.stroke_round(Rect::new(r.x + 14, r.y + 13, 20, 20), 10, 2, 0x66FF_FFFF);
            text::draw(c, Font::Body, r.x + 46, r.y + 14, &text::fit(Font::Body, &t, r.w - 60), TEXT);
            y += 54;
        }
    }

    fn task_rows(m: Rect, n: usize) -> Vec<Rect> {
        (0..n).map(|i| Rect::new(m.x + 56, m.y + 170 + i as i32 * 54, m.w - 112, 46)).collect()
    }

    fn entry_rect(m: Rect) -> Rect {
        Rect::new(m.x + 56, m.y + 108, m.w - 112, 46)
    }

    fn draw_tasks(&mut self, c: &mut Canvas, m: Rect, focused: bool) {
        text::draw(c, Font::Title, m.x + 56, m.y + 50, "Task", TEXT);
        ui::text_field(c, Self::entry_rect(m), &self.entry, "Nuovo task, poi Invio", self.typing && focused);
        for (r, (done, t)) in Self::task_rows(m, self.tasks.len()).into_iter().zip(self.tasks.iter()) {
            if r.y > m.bottom() - 50 {
                break;
            }
            c.fill_round(r, 14, if self.hover == Some(r) { ui::PANEL_HOVER } else { ui::PANEL });
            let b = Rect::new(r.x + 14, r.y + 13, 20, 20);
            if *done {
                c.fill_circle(b.x + 10, b.y + 10, 10, 0xFFFF_FFFF);
                c.line(b.x + 5, b.y + 10, b.x + 9, b.y + 14, 2, 0xFF0E_0B0C);
                c.line(b.x + 9, b.y + 14, b.x + 15, b.y + 6, 2, 0xFF0E_0B0C);
            } else {
                c.stroke_round(b, 10, 2, 0x66FF_FFFF);
            }
            text::draw(c, Font::Body, r.x + 46, r.y + 14, &text::fit(Font::Body, t, r.w - 60), if *done { TEXT_FAINT } else { TEXT });
        }
    }

    fn note_cards(m: Rect, n: usize) -> Vec<Rect> {
        let cols = ((m.w - 112) / 240).max(1);
        let w = (m.w - 112 - (cols - 1) * 14) / cols;
        (0..n).map(|i| Rect::new(m.x + 56 + (i as i32 % cols) * (w + 14), m.y + 120 + (i as i32 / cols) * 134, w, 120)).collect()
    }

    fn new_note_btn(m: Rect) -> Rect {
        Rect::new(m.right() - 56 - 150, m.y + 46, 150, 42)
    }

    fn draw_notes(&mut self, c: &mut Canvas, m: Rect) {
        text::draw(c, Font::Title, m.x + 56, m.y + 50, "Note", TEXT);
        let nb = Self::new_note_btn(m);
        ui::button(c, nb, "+  Nuova nota", true, self.hover == Some(nb));
        if self.notes.is_empty() {
            text::draw(c, Font::Body, m.x + 60, m.y + 130, "Nessuna nota: creane una.", TEXT_DIM);
        }
        for (r, n) in Self::note_cards(m, self.notes.len()).into_iter().zip(self.notes.iter()) {
            if r.y > m.bottom() - 40 {
                break;
            }
            c.fill_round(r, 18, if self.hover == Some(r) { ui::PANEL_HOVER } else { ui::PANEL });
            c.stroke_round(r, 18, 1, ui::BORDER);
            text::draw(c, Font::Strong, r.x + 18, r.y + 18, &text::fit(Font::Strong, &n.title, r.w - 36), TEXT);
            for (k, l) in text::wrap(Font::Small, &n.preview, r.w - 36).iter().take(2).enumerate() {
                text::draw(c, Font::Small, r.x + 18, r.y + 46 + k as i32 * 18, l, TEXT_DIM);
            }
            let d = time::from_epoch(n.modified);
            text::draw(c, Font::Small, r.x + 18, r.bottom() - 28, &format!("{} {}", d.day, crate::gui::desktop::MONTHS[(d.month as usize + 11) % 12]), TEXT_FAINT);
        }
    }

    fn cal_cells(m: Rect, y0: u64, m0: u64) -> Vec<((u64, u64, u64), Rect, bool)> {
        let first = weekday(y0, m0, 1) as i64;
        let start = time::to_epoch(y0, m0, 1, 12, 0, 0) as i64 - first * 86_400;
        let w = (m.w - 112) / 7;
        let h = 50;
        (0..35)
            .map(|i| {
                let d = time::from_epoch((start + i * 86_400) as u64);
                let r = Rect::new(m.x + 56 + (i as i32 % 7) * w, m.y + 196 + (i as i32 / 7) * (h + 3), w - 3, h);
                ((d.year, d.month, d.day), r, d.month == m0)
            })
            .collect()
    }

    fn cal_buttons(m: Rect) -> [Rect; 3] {
        let x = m.right() - 56 - 124;
        [Rect::new(x, m.y + 52, 36, 36), Rect::new(x + 44, m.y + 52, 36, 36), Rect::new(x + 88, m.y + 52, 36, 36)]
    }

    fn view_tabs(m: Rect) -> Rect {
        Rect::new(m.x + 56, m.y + 112, 280, 48)
    }

    fn new_event_btn(m: Rect) -> Rect {
        Rect::new(m.right() - 56 - 170, m.bottom() - 70, 170, 46)
    }

    fn draw_calendar(&mut self, c: &mut Canvas, m: Rect, focused: bool) {
        let (y0, m0) = self.month;
        let title = format!("{} {y0}", capital(crate::gui::desktop::MONTHS[(m0 as usize + 11) % 12]));
        text::draw(c, Font::Title, m.x + 56, m.y + 50, &title, TEXT);
        let [prev, todayb, next] = Self::cal_buttons(m);
        ui::icon_button(c, prev, "g-back", self.hover == Some(prev));
        ui::icon_button(c, todayb, "calendar", self.hover == Some(todayb));
        ui::icon_button(c, next, "g-forward", self.hover == Some(next));
        ui::segmented(c, Self::view_tabs(m), &["Mese", "Settimana", "Giorno"], self.view);
        let t = today();
        let list_top;
        if self.view == 0 {
            let w = (m.w - 112) / 7;
            for (i, h) in ["L", "M", "M", "G", "V", "S", "D"].iter().enumerate() {
                let x = m.x + 56 + i as i32 * w + w / 2;
                ui::caps(c, x - ui::caps_width(h) / 2, m.y + 176, h, TEXT_FAINT);
            }
            let cells = Self::cal_cells(m, y0, m0);
            for (d, r, inside) in &cells {
                let sel = *d == self.day;
                let is_today = *d == t;
                if sel || is_today && self.day == t {
                    c.fill_round(r.grow(2), 16, 0xFFFF_FFFF);
                } else {
                    c.fill_round(*r, 10, if self.hover == Some(*r) { 0x40FF_FFFF } else if *inside { 0x24FF_FFFF } else { 0x12FF_FFFF });
                }
                let color = if sel || is_today && self.day == t {
                    0xFF0E_0B0C
                } else if *inside {
                    TEXT
                } else {
                    TEXT_FAINT
                };
                let font = if sel { Font::Heading } else { Font::Strong };
                let (lh, _) = text::metrics(font);
                text::draw_centered(c, font, r.x + r.w / 2, r.y + (r.h - lh) / 2 - 4, &format!("{}", d.2), color);
                if !self.events_on(*d).is_empty() {
                    c.fill_circle(r.x + r.w / 2, r.bottom() - 10, 3, color);
                }
            }
            list_top = cells.last().map(|x| x.1.bottom()).unwrap_or(m.y + 400) + 24;
        } else {
            list_top = m.y + 190;
        }
        // The selected day (or week).
        let days: Vec<(u64, u64, u64)> = if self.view == 1 {
            let wd = weekday(self.day.0, self.day.1, self.day.2) as i64;
            let start = time::to_epoch(self.day.0, self.day.1, self.day.2, 12, 0, 0) as i64 - wd * 86_400;
            (0..7).map(|i| time::from_epoch((start + i * 86_400) as u64)).map(|d| (d.year, d.month, d.day)).collect()
        } else {
            alloc::vec![self.day]
        };
        let mut y = list_top;
        for d in &days {
            let caption = format!("{} {} {}", WEEKDAYS[weekday(d.0, d.1, d.2)], d.2, crate::gui::desktop::MONTHS[(d.1 as usize + 11) % 12]);
            ui::caps(c, m.x + 60, y, &caption, TEXT_FAINT);
            y += 24;
            let evs = self.events_on(*d);
            if evs.is_empty() && days.len() > 1 {
                text::draw(c, Font::Small, m.x + 60, y, "Nessun evento", TEXT_FAINT);
                y += 26;
            }
            for e in evs {
                if y > m.bottom() - 90 {
                    break;
                }
                let r = Rect::new(m.x + 56, y, m.w - 112 - 190, 40);
                c.fill_round(r, 12, ui::PANEL);
                c.fill_round(Rect::new(r.x + 12, r.y + 10, 4, 20), 2, ui::ACCENT);
                text::draw(c, Font::Strong, r.x + 26, r.y + 11, &text::fit(Font::Strong, &e.text, r.w - 120), TEXT);
                text::draw_right(c, Font::Small, r.right() - 14, r.y + 13, if e.time.is_empty() { "Tutto il giorno" } else { &e.time }, TEXT_DIM);
                y += 46;
            }
        }
        let nb = Self::new_event_btn(m);
        if days.len() == 1 && self.events_on(self.day).is_empty() && !self.typing {
            // The empty day, centred in the room left above the button.
            let cx = m.x + m.w / 2;
            let top = list_top + 20;
            if nb.y - top >= 150 {
                let cy = top + (nb.y - top - 150) / 2;
                c.fill_circle(cx, cy + 36, 36, 0x1AFF_FFFF);
                crate::gui::res::draw_icon(c, "calendar", 32, cx - 16, cy + 20);
                text::draw_centered(c, Font::Heading, cx, cy + 86, "Giornata libera", TEXT);
                text::draw_centered(c, Font::Small, cx, cy + 116, "Nessun evento per questo giorno.", TEXT_DIM);
            }
        }
        if self.typing {
            let r = Rect::new(m.x + 56, nb.y, m.w - 112, 46);
            ui::text_field(c, r, &self.entry, "9:30 Riunione, poi Invio", focused);
        } else {
            ui::button(c, nb, "+  Nuovo evento", true, self.hover == Some(nb));
        }
    }

    fn draw_profile(&mut self, c: &mut Canvas, m: Rect) {
        text::draw(c, Font::Title, m.x + 56, m.y + 50, "Profilo", TEXT);
        let Some(a) = self.account.clone() else {
            text::draw(c, Font::Body, m.x + 56, m.y + 110, "Accedi con il tuo account ticly per vedere il profilo.", TEXT_DIM);
            return;
        };
        let card = Rect::new(m.x + 56, m.y + 108, m.w - 112, 150);
        c.fill_round(card, 22, ui::PANEL);
        tui::avatar(c, card.x + 70, card.y + 75, 44, &a.initials());
        text::draw(c, Font::Title, card.x + 136, card.y + 34, &text::fit(Font::Title, &a.name, card.w - 160), TEXT);
        text::draw(c, Font::Body, card.x + 136, card.y + 80, &a.email, TEXT_DIM);
        let plan = format!("Piano {}", a.plan);
        let pw = ui::caps_width(&plan) + 24;
        c.fill_round(Rect::new(card.x + 136, card.y + 108, pw, 24), 12, 0x40E0_662E);
        ui::caps(c, card.x + 148, card.y + 115, &plan, TEXT);
        let created = {
            let d = time::from_epoch(a.created.saturating_sub(crate::linux::UNIX_2000));
            format!("{} {} {}", d.day, crate::gui::desktop::MONTHS[(d.month as usize + 11) % 12], d.year)
        };
        let rows = [("Account", String::from("Su questo computer")), ("ID account", a.id.clone()), ("Creato il", created)];
        let mut y = card.bottom() + 24;
        for (k, v) in rows {
            let r = Rect::new(m.x + 56, y, m.w - 112, 50);
            c.fill_round(r, 14, ui::PANEL);
            text::draw(c, Font::Body, r.x + 18, r.y + 15, k, TEXT_DIM);
            text::draw_right(c, Font::Strong, r.right() - 18, r.y + 15, &v, TEXT);
            y += 58;
        }
        text::draw(c, Font::Small, m.x + 56, y + 10, "I tuoi dati (attività, note, calendario) restano nei file di questo computer.", TEXT_FAINT);
    }
}

fn capital(s: &str) -> String {
    let mut ch = s.chars();
    ch.next().map(|f| f.to_uppercase().chain(ch).collect()).unwrap_or_default()
}

impl App for TiclyDo {
    fn title(&self) -> String {
        String::from("ticly Do")
    }

    fn icon(&self) -> &str {
        "ticly-do"
    }

    fn kind(&self) -> &'static str {
        "ticly-do"
    }

    fn size(&self) -> (i32, i32) {
        (1060, 680)
    }

    fn resizable(&self) -> bool {
        true
    }

    fn sidebar(&self) -> i32 {
        if self.login.is_some() {
            0
        } else {
            SIDEBAR
        }
    }

    fn draw(&mut self, c: &mut Canvas, area: Rect, blur: &Canvas, focused: bool) {
        if let Some(l) = &mut self.login {
            l.draw(c, blur, area, "Do");
            return;
        }
        self.draw_sidebar(c, area);
        let m = Self::main(area);
        let saved = c.clip();
        c.set_clip(saved.intersect(&m));
        match self.section {
            0 => self.draw_overview(c, m),
            1 => self.draw_tasks(c, m, focused),
            2 => self.draw_notes(c, m),
            3 => self.draw_calendar(c, m, focused),
            _ => self.draw_profile(c, m),
        }
        c.set_clip(saved);
    }

    fn press(&mut self, x: i32, y: i32, _double: bool, area: Rect) -> Action {
        let (ax, ay) = (area.x + x, area.y + y);
        if let Some(l) = &mut self.login {
            l.press(ax, ay, area);
            return Action::Redraw;
        }
        for i in 0..SECTIONS.len() {
            if Self::nav(area, i).contains(ax, ay) {
                self.section = i;
                self.typing = false;
                self.entry.clear();
                self.tasks = load_tasks();
                self.notes = load_notes();
                self.events = load_events();
                return Action::Redraw;
            }
        }
        if Self::exit_rect(area).contains(ax, ay) {
            if self.account.is_some() {
                ticly::sign_out();
                self.account = None;
            } else {
                self.login = Some(Login::new());
            }
            return Action::Redraw;
        }
        let m = Self::main(area);
        match self.section {
            1 => {
                self.typing = Self::entry_rect(m).contains(ax, ay);
                for (i, r) in Self::task_rows(m, self.tasks.len()).into_iter().enumerate() {
                    if r.contains(ax, ay) {
                        self.tasks[i].0 = !self.tasks[i].0;
                        save_tasks(&self.tasks);
                    }
                }
            }
            2 => {
                if Self::new_note_btn(m).contains(ax, ay) {
                    let _ = fs::abs(NOTES).map(|p| if !fs::exists(&p) { let _ = fs::make_dir(&p); });
                    let path = super::textedit::free_name(NOTES, "Nota", "txt");
                    if let Ok(p) = fs::abs(&path) {
                        let _ = fs::write_file(&p, b"Nuova nota\n\n");
                    }
                    self.notes = load_notes();
                    return Action::OpenFile(path);
                }
                for (r, n) in Self::note_cards(m, self.notes.len()).into_iter().zip(self.notes.iter()) {
                    if r.contains(ax, ay) {
                        return Action::OpenFile(n.path.clone());
                    }
                }
            }
            3 => {
                let [prev, todayb, next] = Self::cal_buttons(m);
                let (y0, m0) = self.month;
                if prev.contains(ax, ay) {
                    self.month = if m0 == 1 { (y0 - 1, 12) } else { (y0, m0 - 1) };
                } else if next.contains(ax, ay) {
                    self.month = if m0 == 12 { (y0 + 1, 1) } else { (y0, m0 + 1) };
                } else if todayb.contains(ax, ay) {
                    let t = today();
                    self.month = (t.0, t.1);
                    self.day = t;
                } else if Self::view_tabs(m).contains(ax, ay) {
                    for (i, s) in ui::segments(Self::view_tabs(m), 3).iter().enumerate() {
                        if s.contains(ax, ay) {
                            self.view = i;
                        }
                    }
                } else if !self.typing && Self::new_event_btn(m).contains(ax, ay) {
                    self.typing = true;
                    self.entry.clear();
                } else if self.view == 0 {
                    for (d, r, _) in Self::cal_cells(m, y0, m0) {
                        if r.contains(ax, ay) {
                            self.day = d;
                            self.typing = false;
                        }
                    }
                }
            }
            _ => {}
        }
        Action::Redraw
    }

    fn hover(&mut self, at: Option<(i32, i32)>, area: Rect) -> bool {
        if let Some(l) = &mut self.login {
            let t = l.hover_targets(area);
            let h = at.and_then(|(x, y)| t.into_iter().find(|r| r.contains(area.x + x, area.y + y)));
            let ch = h != l.hover;
            l.hover = h;
            return ch;
        }
        let m = Self::main(area);
        let mut t: Vec<Rect> = (0..SECTIONS.len()).map(|i| Self::nav(area, i)).collect();
        t.push(Self::exit_rect(area));
        match self.section {
            1 => t.extend(Self::task_rows(m, self.tasks.len())),
            2 => {
                t.push(Self::new_note_btn(m));
                t.extend(Self::note_cards(m, self.notes.len()));
            }
            3 => {
                t.extend(Self::cal_buttons(m));
                t.push(Self::new_event_btn(m));
                if self.view == 0 {
                    t.extend(Self::cal_cells(m, self.month.0, self.month.1).into_iter().map(|x| x.1));
                }
            }
            _ => {}
        }
        let h = at.and_then(|(x, y)| t.into_iter().find(|r| r.contains(area.x + x, area.y + y)));
        let ch = h != self.hover;
        self.hover = h;
        ch
    }

    fn key(&mut self, ev: KeyEvent) -> Action {
        if let Some(l) = &mut self.login {
            if ev.key == Key::Escape {
                self.login = None;
                return Action::Redraw;
            }
            return if l.key(ev) { Action::Redraw } else { Action::None };
        }
        if ev.key == Key::Escape {
            self.typing = false;
            self.entry.clear();
            return Action::Redraw;
        }
        if self.section == 1 && ev.key == Key::Enter && !self.entry.trim().is_empty() {
            self.tasks.insert(0, (false, self.entry.trim().to_string()));
            save_tasks(&self.tasks);
            self.entry.clear();
            return Action::Redraw;
        }
        if self.section == 3 && self.typing && ev.key == Key::Enter {
            let t = self.entry.trim().to_string();
            if !t.is_empty() {
                let (time_part, text_part) = match t.split_once(' ') {
                    Some((a, b)) if a.contains([':', '.']) && a.chars().all(|c| c.is_ascii_digit() || c == ':' || c == '.') => (a.replace('.', ":"), b.to_string()),
                    _ => (String::new(), t.clone()),
                };
                self.events.push(Event { date: self.day, time: time_part, text: text_part });
                save_events(&self.events);
            }
            self.typing = false;
            self.entry.clear();
            return Action::Redraw;
        }
        if (self.section == 1 || (self.section == 3 && self.typing)) && ui::edit_text(&mut self.entry, &ev, 120) {
            if self.section == 1 {
                self.typing = true;
            }
            return Action::Redraw;
        }
        Action::None
    }

    fn tick(&mut self) -> bool {
        if let Some(l) = &mut self.login {
            if let Some(a) = l.poll() {
                self.account = Some(a);
                self.login = None;
            }
            return true;
        }
        false
    }
}
