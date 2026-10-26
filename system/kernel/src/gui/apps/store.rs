//! Negozio: the programs of `sys:/store`, by category, each with a page of
//! its own; one click installs, opens or removes them.

use crate::dev::input::{Key, KeyEvent};
use crate::gui::gfx::{Canvas, Rect};
use crate::gui::res;
use crate::gui::text::{self, Font};
use crate::gui::ui::{self, Anim, TEXT, TEXT_DIM, TEXT_FAINT};
use crate::gui::wm::{Action, App};
use crate::store::{self, PackageInfo};
use crate::util::Size;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

const SIDEBAR: i32 = 196;
const SECTIONS: [(&str, &str); 5] =
    [("Scopri", "g-grid"), ("Giochi", "games"), ("Creatività", "photos"), ("Sviluppatori", "code"), ("Installate", "download")];
const CARD_H: i32 = 92;

pub struct Store {
    packages: Vec<PackageInfo>,
    section: usize,
    /// The package whose page is open.
    page: Option<usize>,
    /// A package being installed (the ring fills), and which.
    installing: Option<(usize, Anim)>,
    hover: Option<Rect>,
    scroll: i32,
    search: String,
    searching: bool,
}

fn icon_of(p: &PackageInfo) -> String {
    store::icon(&p.id).unwrap_or_else(|| String::from(if p.gui { "window" } else { "terminal" }))
}

impl Store {
    pub fn new() -> Store {
        Store { packages: store::available(), section: 0, page: None, installing: None, hover: None, scroll: 0, search: String::new(), searching: false }
    }

    /// The packages shown in the current section.
    fn shown(&self) -> Vec<usize> {
        let q = self.search.to_lowercase();
        (0..self.packages.len())
            .filter(|&i| {
                let p = &self.packages[i];
                if !q.is_empty() {
                    return p.name.to_lowercase().contains(&q) || p.about.to_lowercase().contains(&q);
                }
                match self.section {
                    0 => true,
                    1 => p.category == "Giochi",
                    2 => p.category == "Creatività",
                    3 => p.category == "Sviluppatori",
                    _ => store::installed(&p.id).is_some(),
                }
            })
            .collect()
    }

    fn content(area: Rect) -> Rect {
        Rect::new(area.x + SIDEBAR + 8, area.y + 8, area.w - SIDEBAR - 16, area.h - 16)
    }

    fn banner(area: Rect) -> Rect {
        let c = Self::content(area);
        Rect::new(c.x + 8, c.y + 50, c.w - 16, 170)
    }

    fn grid_top(&self, area: Rect) -> i32 {
        let c = Self::content(area);
        if self.section == 0 && self.search.is_empty() {
            Self::banner(area).bottom() + 44
        } else {
            c.y + 56
        }
    }

    fn card(&self, area: Rect, k: usize) -> Rect {
        let c = Self::content(area);
        let cols = if c.w > 640 { 2 } else { 1 };
        let w = (c.w - 16 - (cols - 1) * 12) / cols;
        let (col, row) = (k as i32 % cols, k as i32 / cols);
        Rect::new(c.x + 8 + col * (w + 12), self.grid_top(area) + row * (CARD_H + 12) - self.scroll, w, CARD_H)
    }

    fn get_btn(card: Rect) -> Rect {
        Rect::new(card.right() - 100, card.y + (card.h - 30) / 2, 86, 30)
    }

    fn place(area: Rect, i: usize) -> Rect {
        Rect::new(area.x + 10, area.y + 64 + i as i32 * 38, SIDEBAR - 16, 34)
    }

    fn search_rect(area: Rect) -> Rect {
        Rect::new(area.x + 12, area.y + 14, SIDEBAR - 24, 32)
    }

    fn page_btns(area: Rect) -> [Rect; 3] {
        let c = Self::content(area);
        [Rect::new(c.x + 8, c.y + 8, 34, 34), Rect::new(c.x + 160, c.y + 128, 110, 34), Rect::new(c.x + 280, c.y + 128, 110, 34)]
    }

    /// The state of the button of package `i`: label, primary.
    fn button_state(&self, i: usize) -> (&'static str, bool) {
        if self.installing.as_ref().is_some_and(|(k, _)| *k == i) {
            return ("", false);
        }
        match store::installed(&self.packages[i].id) {
            Some(_) => ("Apri", false),
            None => ("Ottieni", true),
        }
    }

    fn draw_button(&self, c: &mut Canvas, r: Rect, i: usize) {
        if let Some((k, a)) = &self.installing {
            if *k == i {
                let (cx, cy) = (r.x + r.w / 2, r.y + r.h / 2);
                c.arc(cx, cy, 14, 3, 1000, 0x40FF_FFFF);
                c.arc(cx, cy, 14, 3, a.out().max(10), ui::ACCENT);
                c.fill(Rect::new(cx - 4, cy - 4, 8, 8), ui::ACCENT);
                return;
            }
        }
        let (label, primary) = self.button_state(i);
        let hover = self.hover == Some(r);
        if primary {
            ui::button_color(c, r, label, if hover { 0xFF40_9CFF } else { ui::ACCENT }, 0xFFFF_FFFF);
        } else {
            ui::button(c, r, label, false, hover);
        }
    }

    /// The button of package `i` was pressed.
    fn act(&mut self, i: usize) -> Action {
        if self.installing.is_some() {
            return Action::None;
        }
        let p = &self.packages[i];
        if store::installed(&p.id).is_some() {
            return if p.gui {
                Action::Spawn(store::program_path(p))
            } else {
                Action::Run(p.command.clone())
            };
        }
        self.installing = Some((i, Anim::new(1100)));
        Action::Redraw
    }

    fn finish_install(&mut self) -> Option<(String, String)> {
        let (i, _) = self.installing.take()?;
        let p = self.packages[i].clone();
        match store::install(&p.id) {
            Ok(_) => {
                let how = if p.gui { "La trovi tra le applicazioni." } else { "Si usa nel Terminale." };
                Some((format!("{} installata", p.name), String::from(how)))
            }
            Err(e) => Some((p.name.clone(), format!("Installazione non riuscita: {e}"))),
        }
    }

    fn draw_page(&mut self, c: &mut Canvas, area: Rect, i: usize) {
        let ct = Self::content(area);
        let p = self.packages[i].clone();
        let [back, get, remove] = Self::page_btns(area);
        ui::icon_button(c, back, "g-back", self.hover == Some(back));
        res::draw_icon(c, &icon_of(&p), 128, ct.x + 16, ct.y + 50);
        text::draw(c, Font::Title, ct.x + 160, ct.y + 54, &p.name, TEXT);
        text::draw(c, Font::Body, ct.x + 160, ct.y + 94, &format!("{} · {}", p.developer, p.category), TEXT_DIM);
        self.draw_button(c, get, i);
        if store::installed(&p.id).is_some() && self.installing.is_none() {
            ui::button_color(c, remove, "Rimuovi", if self.hover == Some(remove) { 0x66FF_453A } else { 0x40FF_453A }, 0xFFFF_6961);
        }
        c.fill(Rect::new(ct.x + 16, ct.y + 196, ct.w - 32, 1), ui::SEPARATOR);
        // Facts in a row, as on the App Store.
        let facts = [
            ("CATEGORIA", p.category.clone()),
            ("VERSIONE", p.version.clone()),
            ("DIMENSIONI", format!("{}", Size(p.size))),
            ("TIPO", String::from(if !p.native { "Linux" } else if p.gui { "App" } else { "Terminale" })),
        ];
        let fw = (ct.w - 32) / facts.len() as i32;
        for (k, (label, value)) in facts.iter().enumerate() {
            let x = ct.x + 16 + k as i32 * fw + fw / 2;
            text::draw_centered(c, Font::Small, x, ct.y + 210, label, TEXT_FAINT);
            text::draw_centered(c, Font::Strong, x, ct.y + 230, &text::fit(Font::Strong, value, fw - 10), TEXT_DIM);
            if k > 0 {
                c.fill(Rect::new(ct.x + 16 + k as i32 * fw, ct.y + 212, 1, 36), ui::SEPARATOR);
            }
        }
        c.fill(Rect::new(ct.x + 16, ct.y + 264, ct.w - 32, 1), ui::SEPARATOR);
        text::draw(c, Font::Heading, ct.x + 16, ct.y + 280, "Descrizione", TEXT);
        let body = if p.details.is_empty() { p.about.clone() } else { format!("{}\n\n{}", p.about, p.details.replace("\\n", "\n")) };
        let mut y = ct.y + 314;
        for line in text::wrap(Font::Body, &body, ct.w - 40) {
            if y > ct.bottom() - 20 {
                break;
            }
            text::draw(c, Font::Body, ct.x + 16, y, &line, TEXT_DIM);
            y += 22;
        }
    }
}

impl App for Store {
    fn title(&self) -> String {
        String::from("Negozio")
    }

    fn icon(&self) -> &str {
        "store"
    }

    fn kind(&self) -> &'static str {
        "store"
    }

    fn size(&self) -> (i32, i32) {
        (880, 580)
    }

    fn resizable(&self) -> bool {
        true
    }

    fn sidebar(&self) -> i32 {
        SIDEBAR
    }

    fn draw(&mut self, c: &mut Canvas, area: Rect, _blur: &Canvas, focused: bool) {
        ui::search_field(c, Self::search_rect(area), &self.search, "Cerca", self.searching && focused);
        for (i, (name, icon)) in SECTIONS.iter().enumerate() {
            let r = Self::place(area, i);
            if i == self.section && self.search.is_empty() {
                c.fill_round(r, 9, ui::PANEL_HOVER);
            } else if self.hover == Some(r) {
                c.fill_round(r, 9, ui::PANEL);
            }
            res::draw_icon(c, icon, 22, r.x + 8, r.y + 6);
            text::draw(c, Font::Body, r.x + 40, r.y + 8, name, TEXT);
        }
        let ct = Self::content(area);
        let saved = c.clip();
        c.set_clip(saved.intersect(&ct));
        if let Some(i) = self.page {
            self.draw_page(c, area, i);
            c.set_clip(saved);
            return;
        }
        let title = if self.search.is_empty() { SECTIONS[self.section].0 } else { "Risultati" };
        text::draw(c, Font::Title, ct.x + 12, ct.y + 6, title, TEXT);
        let shown = self.shown();
        if self.section == 0 && self.search.is_empty() {
            // The app of the day.
            let b = Self::banner(area);
            let featured = self.packages.iter().position(|p| p.id == "disegno").or(if self.packages.is_empty() { None } else { Some(0) });
            c.fill_round_grad(b, 20, 0xFF6E_44FF, 0xFFE0_4E9A);
            if let Some(i) = featured {
                let p = &self.packages[i];
                text::draw(c, Font::Small, b.x + 26, b.y + 22, "L'APP DEL GIORNO", 0xCCFF_FFFF);
                text::draw(c, Font::Title, b.x + 26, b.y + 42, &p.name, 0xFFFF_FFFF);
                let lines = text::wrap(Font::Body, &p.about, b.w - 240);
                for (k, l) in lines.iter().take(2).enumerate() {
                    text::draw(c, Font::Body, b.x + 26, b.y + 86 + k as i32 * 22, l, 0xE6FF_FFFF);
                }
                res::draw_icon(c, &icon_of(p), 120, b.right() - 150, b.y + 25);
                let btn = Rect::new(b.x + 26, b.bottom() - 46, 96, 32);
                let (label, _) = self.button_state(i);
                if label.is_empty() {
                    self.draw_button(c, btn, i);
                } else {
                    ui::button_color(c, btn, label, 0xFFFF_FFFF, 0xFF6E_44FF);
                }
            }
            text::draw(c, Font::Heading, ct.x + 12, b.bottom() + 12, "Tutte le app", TEXT);
        }
        if shown.is_empty() {
            let msg = if self.section == 4 { "Nessuna app installata dal Negozio" } else { "Nessuna app" };
            text::draw_centered(c, Font::Body, ct.x + ct.w / 2, self.grid_top(area) + 30, msg, TEXT_FAINT);
        }
        for (k, &i) in shown.iter().enumerate() {
            let r = self.card(area, k);
            if r.bottom() < ct.y || r.y > ct.bottom() {
                continue;
            }
            let p = &self.packages[i];
            c.fill_round(r, 16, if self.hover == Some(r) { ui::PANEL_HOVER } else { ui::PANEL });
            res::draw_icon(c, &icon_of(p), 64, r.x + 12, r.y + 14);
            text::draw(c, Font::Strong, r.x + 88, r.y + 16, &text::fit(Font::Strong, &p.name, r.w - 200), TEXT);
            text::draw(c, Font::Small, r.x + 88, r.y + 38, &p.category, TEXT_DIM);
            text::draw(c, Font::Small, r.x + 88, r.y + 58, &text::fit(Font::Small, &p.about, r.w - 200), TEXT_FAINT);
            self.draw_button(c, Self::get_btn(r), i);
        }
        c.set_clip(saved);
    }

    fn press(&mut self, x: i32, y: i32, _double: bool, area: Rect) -> Action {
        let (ax, ay) = (area.x + x, area.y + y);
        if Self::search_rect(area).contains(ax, ay) {
            self.searching = true;
            self.page = None;
            return Action::Redraw;
        }
        self.searching = false;
        for i in 0..SECTIONS.len() {
            if Self::place(area, i).contains(ax, ay) {
                self.section = i;
                self.page = None;
                self.scroll = 0;
                self.search.clear();
                return Action::Redraw;
            }
        }
        if let Some(i) = self.page {
            let [back, get, remove] = Self::page_btns(area);
            if back.contains(ax, ay) {
                self.page = None;
                return Action::Redraw;
            }
            if get.contains(ax, ay) {
                return self.act(i);
            }
            if remove.contains(ax, ay) && store::installed(&self.packages[i].id).is_some() {
                let name = self.packages[i].name.clone();
                return match store::remove(&self.packages[i].id) {
                    Ok(()) => Action::Notify(format!("{name} rimossa"), String::from("Puoi installarla di nuovo quando vuoi."), "store"),
                    Err(e) => Action::Notify(name, format!("Non si è potuta rimuovere: {e}"), "store"),
                };
            }
            return Action::None;
        }
        if self.section == 0 && self.search.is_empty() {
            let b = Self::banner(area);
            let btn = Rect::new(b.x + 26, b.bottom() - 46, 96, 32);
            if let Some(i) = self.packages.iter().position(|p| p.id == "disegno") {
                if btn.contains(ax, ay) {
                    return self.act(i);
                }
                if b.contains(ax, ay) {
                    self.page = Some(i);
                    return Action::Redraw;
                }
            }
        }
        for (k, &i) in self.shown().iter().enumerate() {
            let r = self.card(area, k);
            if Self::get_btn(r).contains(ax, ay) {
                return self.act(i);
            }
            if r.contains(ax, ay) {
                self.page = Some(i);
                return Action::Redraw;
            }
        }
        Action::Redraw
    }

    fn hover(&mut self, at: Option<(i32, i32)>, area: Rect) -> bool {
        let mut targets: Vec<Rect> = (0..SECTIONS.len()).map(|i| Self::place(area, i)).collect();
        if self.page.is_some() {
            targets.extend(Self::page_btns(area));
        } else {
            for k in 0..self.shown().len() {
                let r = self.card(area, k);
                targets.push(Self::get_btn(r));
                targets.push(r);
            }
        }
        let h = at.and_then(|(x, y)| targets.into_iter().find(|r| r.contains(area.x + x, area.y + y)));
        let changed = h != self.hover;
        self.hover = h;
        changed
    }

    fn key(&mut self, ev: KeyEvent) -> Action {
        if ev.key == Key::Escape {
            self.page = None;
            self.search.clear();
            self.searching = false;
            return Action::Redraw;
        }
        if self.searching && ui::edit_text(&mut self.search, &ev, 40) {
            self.scroll = 0;
            return Action::Redraw;
        }
        Action::None
    }

    fn scroll(&mut self, lines: i32) -> bool {
        if self.page.is_some() {
            return false;
        }
        let old = self.scroll;
        let rows = (self.shown().len() as i32 + 1) / 2 + 2;
        self.scroll = (self.scroll + lines * 40).clamp(0, (rows * (CARD_H + 12) - 200).max(0));
        old != self.scroll
    }

    fn animating(&self) -> bool {
        self.installing.is_some()
    }

    fn frame(&mut self) -> bool {
        if self.installing.as_ref().is_some_and(|(_, a)| a.done()) {
            if let Some((title, body)) = self.finish_install() {
                crate::gui::notify(&title, &body, "store");
            }
        }
        true
    }
}
