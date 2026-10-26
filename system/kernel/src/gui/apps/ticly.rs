//! Pieces the ticly apps share: the wordmark, the avatar, the account card,
//! the AI credits box and the sign-in panel (the look of login.ticly.it).

use crate::dev::input::{Key, KeyEvent};
use crate::gui::gfx::{Canvas, Rect};
use crate::gui::text::{self, Font};
use crate::gui::ui::{self, TEXT, TEXT_DIM, TEXT_FAINT};
use crate::sync::IrqLock;
use crate::ticly::{self, Account};
use alloc::format;
use alloc::string::String;
use alloc::sync::Arc;

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

/// "1.234" from 1234 (Italian thousands).
pub fn thousands(n: i64) -> String {
    let s = format!("{}", n.unsigned_abs());
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push('.');
        }
        out.push(ch);
    }
    if n < 0 {
        out.insert(0, '-');
    }
    out
}

/// The credits box: remaining / max, a green bar, when they renew.
pub fn credits_box(c: &mut Canvas, r: Rect, remaining: i64, max: i64, renew: &str) {
    c.fill_round(r, 16, ui::PANEL);
    text::draw(c, Font::Small, r.x + 14, r.y + 14, "Crediti AI", TEXT_DIM);
    let v = if max > 0 { format!("{} / {}", thousands(remaining), thousands(max)) } else { String::from("Modelli gratuiti") };
    text::draw_right(c, Font::Strong, r.right() - 14, r.y + 12, &v, TEXT);
    let bar = Rect::new(r.x + 14, r.y + 38, r.w - 28, 5);
    c.fill_round(bar, 2, 0x26FF_FFFF);
    let frac = if max > 0 { (remaining.clamp(0, max) * bar.w as i64 / max) as i32 } else { bar.w };
    c.fill_round(Rect::new(bar.x, bar.y, frac.max(4), 5), 2, 0xFF34_D27A);
    if !renew.is_empty() {
        text::draw(c, Font::Small, r.x + 14, r.y + 52, &text::fit(Font::Small, renew, r.w - 28), TEXT_DIM);
    }
}

/// "17 novembre" from "2026-11-17T09:12:00.000Z".
pub fn renew_text(iso: &str) -> String {
    let d: alloc::vec::Vec<u64> = iso.get(..10).unwrap_or("").split('-').filter_map(|p| p.parse().ok()).collect();
    if d.len() != 3 {
        return String::new();
    }
    let month = crate::gui::desktop::MONTHS.get((d[1] as usize).wrapping_sub(1)).copied().unwrap_or("");
    format!("Si rinnovano il {} {month}", d[2])
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
    Email,
    Password,
    Code,
    Token,
}

/// The sign-in panel: e-mail and password through the hub, a second step
/// for the two-factor code, or the token pasted from the callback address.
pub struct Login {
    email: String,
    password: String,
    code: String,
    token: String,
    focus: Field,
    need_code: bool,
    paste: bool,
    message: String,
    result: Arc<IrqLock<Option<Result<Account, String>>>>,
    busy: bool,
    pub hover: Option<Rect>,
}

impl Login {
    pub fn new() -> Login {
        let email = ticly::expired_account().map(|a| a.email).unwrap_or_default();
        let message = if email.is_empty() { String::new() } else { String::from("La sessione è scaduta: accedi di nuovo.") };
        Login {
            focus: if email.is_empty() { Field::Email } else { Field::Password },
            email,
            password: String::new(),
            code: String::new(),
            token: String::new(),
            need_code: false,
            paste: false,
            message,
            result: Arc::new(IrqLock::new(None)),
            busy: false,
            hover: None,
        }
    }

    fn card(area: Rect) -> Rect {
        let w = 420.min(area.w - 40);
        let h = 490.min(area.h - 40);
        Rect::new(area.x + (area.w - w) / 2, area.y + (area.h - h) / 2, w, h)
    }

    fn fields(&self, area: Rect) -> alloc::vec::Vec<(Field, Rect)> {
        let c = Self::card(area);
        let x = c.x + 32;
        let w = c.w - 64;
        if self.paste {
            return alloc::vec![(Field::Token, Rect::new(x, c.y + 206, w, 48))];
        }
        if self.need_code {
            return alloc::vec![(Field::Code, Rect::new(x, c.y + 206, w, 48))];
        }
        alloc::vec![(Field::Email, Rect::new(x, c.y + 188, w, 48)), (Field::Password, Rect::new(x, c.y + 264, w, 48))]
    }

    fn submit_rect(area: Rect) -> Rect {
        let c = Self::card(area);
        Rect::new(c.x + 32, c.y + 336, c.w - 64, 46)
    }

    fn switch_rect(area: Rect) -> Rect {
        let c = Self::card(area);
        Rect::new(c.x + 32, c.y + 394, c.w - 64, 40)
    }

    fn value(&self, f: Field) -> &String {
        match f {
            Field::Email => &self.email,
            Field::Password => &self.password,
            Field::Code => &self.code,
            Field::Token => &self.token,
        }
    }

    pub fn draw(&mut self, c: &mut Canvas, blur: &Canvas, area: Rect, app: &str) {
        let card = Self::card(area);
        ui::card(c, blur, card, 24, ui::GLASS_STRONG);
        wordmark(c, card.x + 32, card.y + 30, app);
        let title = if self.paste {
            "Incolla il token"
        } else if self.need_code {
            "Verifica in due passaggi"
        } else {
            "Accedi a ticly"
        };
        text::draw(c, Font::Title, card.x + 32, card.y + 70, title, TEXT);
        let lead = if self.paste {
            String::from("Accedi su login.ticly.it da un altro dispositivo e incolla qui il token dell'indirizzo finale.")
        } else if self.need_code {
            String::from("Scrivi il codice a sei cifre della tua app di autenticazione.")
        } else {
            String::from("Un unico account per Do, AI, Music e tutto l'ecosistema.")
        };
        for (i, l) in text::wrap(Font::Body, &lead, card.w - 64).iter().take(2).enumerate() {
            text::draw(c, Font::Body, card.x + 32, card.y + 118 + i as i32 * 20, l, TEXT_DIM);
        }
        for (f, r) in self.fields(area) {
            let label = match f {
                Field::Email => "E-mail",
                Field::Password => "Password",
                Field::Code => "Codice",
                Field::Token => "Token",
            };
            ui::caps(c, r.x + 2, r.y - 18, label, TEXT_FAINT);
            let v = self.value(f);
            let shown = if f == Field::Password { "•".repeat(v.chars().count()) } else { v.clone() };
            let placeholder = match f {
                Field::Email => "nome@esempio.it",
                Field::Password => "La tua password",
                Field::Code => "123456",
                Field::Token => "eyJhbGciOi…",
            };
            ui::text_field(c, r, &text::fit(Font::Heading, &shown, r.w - 40), placeholder, self.focus == f);
        }
        let submit = Self::submit_rect(area);
        let label = if self.busy { "Accesso…" } else if self.paste || self.need_code { "Continua" } else { "Accedi" };
        ui::button(c, submit, label, true, self.hover == Some(submit));
        let sw = Self::switch_rect(area);
        ui::button(c, sw, if self.paste { "Accedi con e-mail e password" } else { "Ho già un token" }, false, self.hover == Some(sw));
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
        if Self::submit_rect(area).contains(ax, ay) {
            self.submit();
            return true;
        }
        if Self::switch_rect(area).contains(ax, ay) {
            self.paste = !self.paste;
            self.need_code = false;
            self.message.clear();
            self.focus = if self.paste { Field::Token } else { Field::Email };
            return true;
        }
        false
    }

    fn submit(&mut self) {
        if self.busy {
            return;
        }
        self.message.clear();
        if self.paste {
            match ticly::sign_in_with_token(&self.token) {
                Ok(a) => *self.result.lock() = Some(Ok(a)),
                Err(e) => self.message = translate(&e),
            }
            return;
        }
        if self.email.trim().is_empty() || self.password.is_empty() {
            self.message = String::from("Scrivi e-mail e password.");
            return;
        }
        if !crate::net::online() {
            self.message = String::from("Serve una connessione a internet.");
            return;
        }
        self.busy = true;
        let (email, password, code) = (String::from(self.email.trim()), self.password.clone(), self.need_code.then(|| self.code.clone()));
        let out = self.result.clone();
        crate::task::spawn("ticly-login", move || {
            let r = ticly::sign_in(&email, &password, code.as_deref());
            *out.lock() = Some(r);
        });
    }

    pub fn key(&mut self, ev: KeyEvent) -> bool {
        match ev.key {
            Key::Enter => {
                if self.focus == Field::Email && !self.paste && !self.need_code {
                    self.focus = Field::Password;
                } else {
                    self.submit();
                }
                return true;
            }
            Key::Tab => {
                self.focus = match self.focus {
                    Field::Email => Field::Password,
                    Field::Password => Field::Email,
                    f => f,
                };
                return true;
            }
            _ => {}
        }
        if ev.ctrl && matches!(ev.key, Key::Char('v') | Key::Char('V')) {
            let clip = ui::paste();
            match self.focus {
                Field::Token => self.token.push_str(clip.trim()),
                Field::Email => self.email.push_str(clip.trim()),
                Field::Password => self.password.push_str(&clip),
                Field::Code => self.code.push_str(clip.trim()),
            }
            return true;
        }
        let max = if self.focus == Field::Token { 4000 } else { 200 };
        let f = self.focus;
        let v = match f {
            Field::Email => &mut self.email,
            Field::Password => &mut self.password,
            Field::Code => &mut self.code,
            Field::Token => &mut self.token,
        };
        ui::edit_text(v, &ev, max)
    }

    /// The sign-in ended: the account, or None while it goes on.
    pub fn poll(&mut self) -> Option<Account> {
        let r = self.result.lock().take()?;
        self.busy = false;
        match r {
            Ok(a) => Some(a),
            Err(e) if e == "2fa" => {
                self.need_code = true;
                self.focus = Field::Code;
                self.message.clear();
                None
            }
            Err(e) => {
                self.message = translate(&e);
                None
            }
        }
    }

    pub fn hover_targets(&self, area: Rect) -> alloc::vec::Vec<Rect> {
        alloc::vec![Self::submit_rect(area), Self::switch_rect(area)]
    }
}

/// The hub's English messages, in Italian where they are ours.
fn translate(e: &str) -> String {
    let it = match e {
        "this is not a ticly token" => "Questo non è un token ticly.",
        "this token has expired: sign in again" => "Il token è scaduto: accedi di nuovo.",
        "the ticly hub did not start a session" | "the ticly hub did not hand out a token" => {
            "Il login di ticly non ha risposto come previsto. Puoi accedere da un altro dispositivo e incollare il token."
        }
        _ => "",
    };
    if it.is_empty() {
        format!("Accesso non riuscito: {}", net_error(e))
    } else {
        String::from(it)
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
