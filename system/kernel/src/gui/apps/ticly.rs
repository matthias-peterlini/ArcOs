//! Pieces the ticly apps share: the wordmark, the avatar, the account card
//! and the sign-in panel (the look of login.ticly.it),
//! for the accounts kept on this computer.

use crate::dev::input::{Key, KeyEvent};
use crate::gui::gfx::{Canvas, Rect};
use crate::gui::text::{self, Font};
use crate::gui::ui::{self, TEXT, TEXT_DIM, TEXT_FAINT};
use crate::sync::IrqLock;
use crate::ticly::{self, Account};
use alloc::format;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;

/// "ticly" with the app's name after it ("Ai", "Do").
pub fn wordmark(c: &mut Canvas, x: i32, y: i32, app: &str) -> i32 {
    let w = text::draw(c, Font::Display, x, y, "ticly", TEXT);
    if app.is_empty() {
        return w;
    }
    w + 6 + text::draw(c, Font::Display, x + w + 6, y, app, TEXT)
}

/// A round avatar with initials.
pub fn avatar(c: &mut Canvas, cx: i32, cy: i32, r: i32, initials: &str) {
    c.fill_circle(cx, cy, r, 0xFF5A_5560);
    c.stroke_round(Rect::new(cx - r, cy - r, 2 * r, 2 * r), r, 1, 0x33FF_FFFF);
    let font = if r >= 36 { Font::Title } else { Font::Strong };
    let (lh, _) = text::metrics(font);
    text::draw_centered(c, font, cx, cy - lh / 2, initials, TEXT);
}

/// The account card at the bottom of a sidebar: avatar, name, e-mail.
pub fn user_card(c: &mut Canvas, r: Rect, a: &Account) {
    c.fill_round(r, 16, ui::PANEL);
    avatar(c, r.x + 30, r.y + r.h / 2, 18, &a.initials());
    let name = if a.name.is_empty() { a.email.clone() } else { a.name.clone() };
    text::draw(c, Font::Strong, r.x + 58, r.y + r.h / 2 - 18, &text::fit(Font::Strong, &name, r.w - 70), TEXT);
    text::draw(c, Font::Small, r.x + 58, r.y + r.h / 2 + 2, &text::fit(Font::Small, &a.email, r.w - 70), TEXT_DIM);
}

/// "Esci" in red, with the exit glyph.
pub fn sign_out_row(c: &mut Canvas, r: Rect, label: &str, hover: bool) {
    c.fill_round(r, 14, if hover { ui::PANEL_HOVER } else { ui::PANEL });
    crate::gui::res::draw_icon(c, "g-power", 16, r.x + 16, r.y + (r.h - 16) / 2);
    let (lh, _) = text::metrics(Font::Strong);
    text::draw(c, Font::Strong, r.x + 44, r.y + (r.h - lh) / 2, label, 0xFFFF_6B5E);
}

/// The greeting of the hour.
pub fn greeting() -> &'static str {
    match crate::time::from_epoch(crate::time::now()).hour {
        5..=11 => "Buongiorno",
        12..=17 => "Buon pomeriggio",
        _ => "Buonasera",
    }
}

// ----------------------------------------------------------------------------
// Signing in

#[derive(Clone, Copy, PartialEq, Eq)]
enum Field {
    Name,
    Email,
    Password,
}

/// The sign-in panel: e-mail and password of an account of this computer,
/// or a new account (name, e-mail, password).
pub struct Login {
    name: String,
    email: String,
    password: String,
    focus: Field,
    /// Making a new account.
    create: bool,
    message: String,
    result: Arc<IrqLock<Option<Result<Account, String>>>>,
    busy: bool,
    pub hover: Option<Rect>,
}

impl Login {
    pub fn new() -> Login {
        Login {
            name: String::new(),
            email: String::new(),
            password: String::new(),
            focus: Field::Email,
            create: false,
            message: String::new(),
            result: Arc::new(IrqLock::new(None)),
            busy: false,
            hover: None,
        }
    }

    fn card(&self, area: Rect) -> Rect {
        let w = 420.min(area.w - 40);
        let h = if self.create { 566 } else { 490 }.min(area.h - 40);
        Rect::new(area.x + (area.w - w) / 2, area.y + (area.h - h) / 2, w, h)
    }

    fn fields(&self, area: Rect) -> Vec<(Field, Rect)> {
        let c = self.card(area);
        let (x, w) = (c.x + 32, c.w - 64);
        if self.create {
            alloc::vec![
                (Field::Name, Rect::new(x, c.y + 188, w, 48)),
                (Field::Email, Rect::new(x, c.y + 264, w, 48)),
                (Field::Password, Rect::new(x, c.y + 340, w, 48)),
            ]
        } else {
            alloc::vec![(Field::Email, Rect::new(x, c.y + 188, w, 48)), (Field::Password, Rect::new(x, c.y + 264, w, 48))]
        }
    }

    fn submit_rect(&self, area: Rect) -> Rect {
        let c = self.card(area);
        Rect::new(c.x + 32, c.y + if self.create { 412 } else { 336 }, c.w - 64, 46)
    }

    fn switch_rect(&self, area: Rect) -> Rect {
        let c = self.card(area);
        Rect::new(c.x + 32, c.y + if self.create { 470 } else { 394 }, c.w - 64, 40)
    }

    fn value(&self, f: Field) -> &String {
        match f {
            Field::Name => &self.name,
            Field::Email => &self.email,
            Field::Password => &self.password,
        }
    }

    pub fn draw(&mut self, c: &mut Canvas, blur: &Canvas, area: Rect, app: &str) {
        let card = self.card(area);
        ui::card(c, blur, card, 24, ui::GLASS_STRONG);
        wordmark(c, card.x + 32, card.y + 30, app);
        let title = if self.create { "Crea un account" } else { "Accedi a ticly" };
        text::draw(c, Font::Title, card.x + 32, card.y + 70, title, TEXT);
        let lead = if self.create {
            "L'account e i tuoi dati restano su questo computer."
        } else {
            "Un unico account per Do, AI e il browser, salvato su questo computer."
        };
        for (i, l) in text::wrap(Font::Body, lead, card.w - 64).iter().take(2).enumerate() {
            text::draw(c, Font::Body, card.x + 32, card.y + 118 + i as i32 * 20, l, TEXT_DIM);
        }
        for (f, r) in self.fields(area) {
            let (label, placeholder) = match f {
                Field::Name => ("Nome", "Nome e cognome"),
                Field::Email => ("E-mail", "nome@esempio.it"),
                Field::Password => ("Password", if self.create { "Almeno 6 caratteri" } else { "La tua password" }),
            };
            ui::caps(c, r.x + 2, r.y - 18, label, TEXT_FAINT);
            let v = self.value(f);
            let shown = if f == Field::Password { "•".repeat(v.chars().count()) } else { v.clone() };
            ui::text_field(c, r, &text::fit(Font::Heading, &shown, r.w - 40), placeholder, self.focus == f);
        }
        let submit = self.submit_rect(area);
        let label = if self.busy {
            "Un momento…"
        } else if self.create {
            "Crea l'account"
        } else {
            "Accedi"
        };
        ui::button(c, submit, label, true, self.hover == Some(submit));
        let sw = self.switch_rect(area);
        ui::button(c, sw, if self.create { "Ho già un account" } else { "Crea un account" }, false, self.hover == Some(sw));
        if !self.message.is_empty() {
            let lines = text::wrap(Font::Small, &self.message, card.w - 64);
            for (i, l) in lines.iter().take(2).enumerate() {
                text::draw(c, Font::Small, card.x + 32, card.bottom() - 46 + i as i32 * 18, l, 0xFFFF_8A80);
            }
        }
    }

    pub fn press(&mut self, ax: i32, ay: i32, area: Rect) -> bool {
        for (f, r) in self.fields(area) {
            if r.contains(ax, ay) {
                self.focus = f;
                return true;
            }
        }
        if self.submit_rect(area).contains(ax, ay) {
            self.submit();
            return true;
        }
        if self.switch_rect(area).contains(ax, ay) {
            self.create = !self.create;
            self.message.clear();
            self.focus = if self.create { Field::Name } else { Field::Email };
            return true;
        }
        false
    }

    fn submit(&mut self) {
        if self.busy {
            return;
        }
        self.message.clear();
        if self.email.trim().is_empty() || self.password.is_empty() {
            self.message = String::from("Scrivi e-mail e password.");
            return;
        }
        self.busy = true;
        let (name, email, password, create) = (self.name.clone(), String::from(self.email.trim()), self.password.clone(), self.create);
        let out = self.result.clone();
        // Hashing the password takes a moment.
        crate::task::spawn("ticly-login", move || {
            let r = if create { ticly::sign_up(&name, &email, &password) } else { ticly::sign_in(&email, &password) };
            *out.lock() = Some(r);
        });
    }

    fn order(&self) -> &'static [Field] {
        if self.create {
            &[Field::Name, Field::Email, Field::Password]
        } else {
            &[Field::Email, Field::Password]
        }
    }

    pub fn key(&mut self, ev: KeyEvent) -> bool {
        let order = self.order();
        let at = order.iter().position(|&f| f == self.focus).unwrap_or(0);
        match ev.key {
            Key::Enter => {
                if at + 1 < order.len() {
                    self.focus = order[at + 1];
                } else {
                    self.submit();
                }
                return true;
            }
            Key::Tab => {
                self.focus = order[(at + 1) % order.len()];
                return true;
            }
            _ => {}
        }
        if ev.ctrl && matches!(ev.key, Key::Char('v') | Key::Char('V')) {
            let clip = ui::paste();
            match self.focus {
                Field::Name => self.name.push_str(clip.trim()),
                Field::Email => self.email.push_str(clip.trim()),
                Field::Password => self.password.push_str(&clip),
            }
            return true;
        }
        let v = match self.focus {
            Field::Name => &mut self.name,
            Field::Email => &mut self.email,
            Field::Password => &mut self.password,
        };
        ui::edit_text(v, &ev, 200)
    }

    /// The sign-in ended: the account, or None while it goes on.
    pub fn poll(&mut self) -> Option<Account> {
        let r = self.result.lock().take()?;
        self.busy = false;
        match r {
            Ok(a) => {
                self.password.clear();
                Some(a)
            }
            Err(e) => {
                self.message = e;
                None
            }
        }
    }

    pub fn hover_targets(&self, area: Rect) -> Vec<Rect> {
        alloc::vec![self.submit_rect(area), self.switch_rect(area)]
    }
}

/// A network error in words: the common ones in Italian, the rest as they are.
pub fn net_error(e: &str) -> String {
    let host = e.split_once(": ").map(|(h, _)| h).unwrap_or("ticly");
    if e.contains("host not found") || e.contains("no network") || e.contains("no route") {
        format!("Non riesco a raggiungere {host}: controlla la connessione.")
    } else if e.contains("timed out") {
        format!("{host} non risponde: riprova tra poco.")
    } else if e.contains("refused") || e.contains("reset") {
        format!("{host} ha chiuso la connessione: riprova tra poco.")
    } else if e.contains("certificate") {
        format!("Il certificato di {host} non è valido.")
    } else {
        String::from(e)
    }
}
