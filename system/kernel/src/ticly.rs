//! The ticly account: one login for the whole ecosystem (Do, AI, Music,
//! Enchat…), as `login.ticly.it` gives it to every app.
//!
//! The hub hands an app a JWT (`?token=…` on its callback): it names the
//! user, the plan and the shared AI credits, and is good for 24 hours. arcOS
//! keeps it in `sys:/system/ticly/session` and reads its payload to show who
//! is signed in; it never needs the signing secret (only servers check the
//! signature, as SingleSignOn-integration.md asks).
//!
//! Endpoints (all overridable in `sys:/system/ticly/config.json`):
//!
//! - `login`: the hub's sign-in API, which starts a session (cookie);
//!   `sso_redirect` (`/api/auth/sso-redirect?app=…`) then answers with the
//!   callback carrying the token. If the hub refuses (two-factor, passkeys),
//!   the token can be pasted from the callback address instead.
//! - `models` and `usage` (`/api/ai/models`, `/api/ai/usage`): called with
//!   the token as a Bearer, as AI-GATEWAY.md documents.
//! - `chat`: the ticly AI backend (`ai.ticly.it/api/chat`), which adds the
//!   gateway key on the server and streams the answer as server-sent events
//!   (the route handler of AI-GATEWAY.md §6). The gateway itself is never
//!   called from here: its key must not live on users' computers.

use crate::fs;
use crate::net::http;
use crate::sync::IrqLock;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use arcjson::Value;

const DIR: &str = "sys:/system/ticly";

/// Where to reach ticly.
#[derive(Clone)]
pub struct Endpoints {
    pub login: String,
    pub sso_redirect: String,
    pub app_slug: String,
    pub models: String,
    pub usage: String,
    pub chat: String,
}

impl Default for Endpoints {
    fn default() -> Self {
        Endpoints {
            login: String::from("https://login.ticly.it/api/auth/login"),
            sso_redirect: String::from("https://login.ticly.it/api/auth/sso-redirect"),
            app_slug: String::from("ticly-ai-web"),
            models: String::from("https://login.ticly.it/api/ai/models"),
            usage: String::from("https://login.ticly.it/api/ai/usage"),
            chat: String::from("https://ai.ticly.it/api/chat"),
        }
    }
}

pub fn endpoints() -> Endpoints {
    let mut e = Endpoints::default();
    let Some(v) = read(&format!("{DIR}/config.json")).and_then(|b| arcjson::parse_bytes(&b)) else { return e };
    let set = |field: &mut String, key: &str| {
        if let Some(s) = v.get(key).as_str() {
            *field = s.to_string();
        }
    };
    set(&mut e.login, "login");
    set(&mut e.sso_redirect, "sso_redirect");
    set(&mut e.app_slug, "app_slug");
    set(&mut e.models, "models");
    set(&mut e.usage, "usage");
    set(&mut e.chat, "chat");
    e
}

fn read(path: &str) -> Option<Vec<u8>> {
    fs::abs(path).and_then(|p| fs::read_file(&p)).ok()
}

fn write(path: &str, data: &[u8]) -> Result<(), String> {
    if let Ok(d) = fs::abs(DIR) {
        if !fs::exists(&d) {
            fs::make_dir(&d).map_err(|e| e.to_string())?;
        }
    }
    fs::abs(path).and_then(|p| fs::write_file(&p, data)).map_err(|e| e.to_string())
}

/// Who is signed in, from the token's payload.
#[derive(Clone, Default, Debug)]
pub struct Account {
    pub id: String,
    pub email: String,
    pub name: String,
    pub plan: String,
    pub ai_tokens: i64,
    /// When the token stops working (seconds since 1970).
    pub expires: u64,
    pub token: String,
}

impl Account {
    /// "Matthias" from "Matthias Peterlini" (or the e-mail's first part).
    pub fn first_name(&self) -> String {
        let n = self.name.split_whitespace().next().unwrap_or("");
        if !n.is_empty() {
            return n.to_string();
        }
        self.email.split('@').next().unwrap_or("").to_string()
    }

    /// "MP" for the avatar.
    pub fn initials(&self) -> String {
        let s: String = self.name.split_whitespace().take(2).filter_map(|w| w.chars().next()).collect();
        if s.is_empty() {
            self.email.chars().take(1).collect::<String>().to_uppercase()
        } else {
            s.to_uppercase()
        }
    }

    pub fn expired(&self) -> bool {
        self.expires != 0 && crate::time::unix_now() >= self.expires
    }
}

fn b64url(s: &str) -> Option<Vec<u8>> {
    let val = |c: u8| -> Option<u32> {
        Some(match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'-' | b'+' => 62,
            b'_' | b'/' => 63,
            _ => return None,
        } as u32)
    };
    let bytes: Vec<u8> = s.bytes().filter(|&c| c != b'=').collect();
    let mut out = Vec::with_capacity(bytes.len() * 3 / 4);
    for ch in bytes.chunks(4) {
        let mut v = 0u32;
        for (i, &c) in ch.iter().enumerate() {
            v |= val(c)? << (18 - 6 * i);
        }
        out.push((v >> 16) as u8);
        if ch.len() > 2 {
            out.push((v >> 8) as u8);
        }
        if ch.len() > 3 {
            out.push(v as u8);
        }
    }
    Some(out)
}

/// Reads a ticly JWT's payload (the signature is the servers' business).
pub fn decode(token: &str) -> Option<Account> {
    let token = token.trim();
    let mut parts = token.split('.');
    let (_h, p, _s) = (parts.next()?, parts.next()?, parts.next()?);
    let v = arcjson::parse_bytes(&b64url(p)?)?;
    let sub = v.get("sub").as_str().or(v.get("id").as_str())?.to_string();
    Some(Account {
        id: sub,
        email: v.get("email").str().to_string(),
        name: v.get("name").str().to_string(),
        plan: v.get("subscription").as_str().unwrap_or("FREE").to_string(),
        ai_tokens: v.get("aiTokens").as_i64().unwrap_or(0),
        expires: v.get("exp").as_f64().unwrap_or(0.0) as u64,
        token: token.to_string(),
    })
}

static SESSION: IrqLock<Option<Option<Account>>> = IrqLock::new(None);

/// The signed-in account, if any (and its token not expired).
pub fn account() -> Option<Account> {
    if let Some(a) = SESSION.lock().clone() {
        return a.filter(|a| !a.expired());
    }
    let a = read(&format!("{DIR}/session")).and_then(|b| decode(&String::from_utf8_lossy(&b)));
    *SESSION.lock() = Some(a.clone());
    a.filter(|a| !a.expired())
}

/// The account whose token just expired (to say "sign in again").
pub fn expired_account() -> Option<Account> {
    let a = read(&format!("{DIR}/session")).and_then(|b| decode(&String::from_utf8_lossy(&b)))?;
    a.expired().then_some(a)
}

/// Keeps a token (pasted or received) after checking it reads as one.
pub fn sign_in_with_token(token: &str) -> Result<Account, String> {
    let token = token.trim().trim_start_matches("token=");
    // A whole callback address works too.
    let token = token.split("token=").last().unwrap_or(token).split('&').next().unwrap_or(token);
    let a = decode(token).ok_or("this is not a ticly token")?;
    if a.expired() {
        return Err("this token has expired: sign in again".to_string());
    }
    write(&format!("{DIR}/session"), a.token.as_bytes())?;
    *SESSION.lock() = Some(Some(a.clone()));
    crate::info!("ticly", "signed in as {}", a.email);
    Ok(a)
}

pub fn sign_out() {
    if let Ok(p) = fs::abs(&format!("{DIR}/session")) {
        let _ = fs::remove(&p);
    }
    *SESSION.lock() = Some(None);
}

/// Signs in with e-mail and password through the hub. `code` is the
/// six-digit code when the account uses two-factor authentication.
pub fn sign_in(email: &str, password: &str, code: Option<&str>) -> Result<Account, String> {
    let e = endpoints();
    let mut body = alloc::vec![("email", Value::string(email)), ("password", Value::string(password))];
    if let Some(c) = code {
        body.push(("code", Value::string(c)));
        body.push(("totp", Value::string(c)));
    }
    let body = Value::object(body).to_string();
    let req = http::Request {
        method: "POST",
        url: &e.login,
        headers: &[("Content-Type", "application/json"), ("Accept", "application/json")],
        body: Some(body.as_bytes()),
    };
    let r = http::fetch_once(&req)?;
    let reply = arcjson::parse_bytes(&r.body);
    // A token in the answer itself.
    if let Some(t) = reply.as_ref().and_then(|v| v.get("token").as_str().map(String::from)) {
        return sign_in_with_token(&t);
    }
    if r.status == 401 || r.status == 400 {
        let msg = reply.as_ref().map(|v| v.get("error").str().to_string()).filter(|m| !m.is_empty());
        return Err(msg.unwrap_or_else(|| "e-mail o password errati".to_string()));
    }
    if reply.as_ref().is_some_and(|v| v.get("requires2FA").as_bool() == Some(true) || v.get("twoFactorRequired").as_bool() == Some(true)) {
        return Err("2fa".to_string());
    }
    if !(200..400).contains(&r.status) {
        return Err(format!("the ticly hub answered {}", r.status));
    }
    // A session cookie: the hub's transparent redirect gives the token.
    let cookies: Vec<String> = r
        .headers
        .iter()
        .filter(|(k, _)| k.eq_ignore_ascii_case("set-cookie"))
        .filter_map(|(_, v)| v.split(';').next().map(String::from))
        .collect();
    if cookies.is_empty() {
        return Err("the ticly hub did not start a session".to_string());
    }
    let cookie = cookies.join("; ");
    let url = format!("{}?app={}", e.sso_redirect, e.app_slug);
    let req = http::Request { method: "GET", url: &url, headers: &[("Cookie", &cookie)], body: None };
    let r = http::fetch_once(&req)?;
    let location = r.header("location").unwrap_or("").to_string();
    if location.contains("token=") {
        return sign_in_with_token(&location);
    }
    Err("the ticly hub did not hand out a token".to_string())
}

fn bearer_json(url: &str, token: &str) -> Result<Value, String> {
    let auth = format!("Bearer {token}");
    let req = http::Request { method: "GET", url, headers: &[("Authorization", &auth), ("Accept", "application/json")], body: None };
    let r = http::fetch(&req, &mut |_, _| true)?;
    if r.status == 401 {
        return Err("session expired: sign in again".to_string());
    }
    if !r.ok() {
        return Err(format!("ticly answered {}", r.status));
    }
    arcjson::parse_bytes(&r.body).ok_or_else(|| "ticly sent something unreadable".to_string())
}

/// AI credits: (remaining, max, plan, next reset).
pub fn usage(token: &str) -> Result<(i64, i64, String, String), String> {
    let v = bearer_json(&endpoints().usage, token)?;
    Ok((
        v.get("remaining").as_i64().unwrap_or(0),
        v.get("max").as_i64().unwrap_or(0),
        v.get("plan").str().to_string(),
        v.get("nextReset").str().to_string(),
    ))
}

/// A model of the catalog.
#[derive(Clone, Debug)]
pub struct Model {
    pub id: String,
    pub name: String,
    pub min_plan: String,
    pub unlocked: bool,
}

/// The models the user's plan can use (and the others, locked).
pub fn models(token: &str) -> Result<Vec<Model>, String> {
    let v = bearer_json(&endpoints().models, token)?;
    Ok(v.get("models")
        .as_array()
        .iter()
        .map(|m| Model {
            id: m.get("id").str().to_string(),
            name: m.get("name").as_str().unwrap_or(m.get("id").str()).to_string(),
            min_plan: m.get("minPlan").str().to_string(),
            unlocked: m.get("unlocked").as_bool().unwrap_or(true),
        })
        .collect())
}

/// The free models, when the catalog cannot be fetched.
pub fn default_models() -> Vec<Model> {
    [("openrouter/free", "Free Auto"), ("google/gemma-4-31b-it:free", "Gemma 4 31B"), ("nvidia/nemotron-3-super-120b-a12b:free", "Nemotron 3 Super")]
        .iter()
        .map(|(id, name)| Model { id: id.to_string(), name: name.to_string(), min_plan: String::from("FREE"), unlocked: true })
        .collect()
}

/// What streaming an answer produced.
pub enum Chunk<'a> {
    /// Text of the answer.
    Text(&'a str),
    /// The model's reasoning (shown apart, or not at all).
    Reasoning,
}

/// Asks ticly AI. `messages` are (role, text); the answer arrives through
/// `on_chunk` as it is written. Returns the model actually used.
pub fn chat(token: &str, model: &str, messages: &[(String, String)], on_chunk: &mut dyn FnMut(Chunk<'_>) -> bool) -> Result<String, String> {
    let e = endpoints();
    let msgs: Vec<Value> = messages.iter().map(|(r, t)| Value::object(alloc::vec![("role", Value::string(r)), ("content", Value::string(t))])).collect();
    let body = Value::object(alloc::vec![("model", Value::string(model)), ("messages", Value::Arr(msgs)), ("stream", Value::Bool(true))]).to_string();
    if body.len() > 8000 {
        return Err("the conversation is too long for one request (8000 characters)".to_string());
    }
    let cookie = format!("ticly_token={token}");
    let auth = format!("Bearer {token}");
    let req = http::Request {
        method: "POST",
        url: &e.chat,
        headers: &[("Content-Type", "application/json"), ("Accept", "text/event-stream"), ("Cookie", &cookie), ("Authorization", &auth)],
        body: Some(body.as_bytes()),
    };
    let mut line = Vec::new();
    let mut got_text = false;
    let mut error: Option<String> = None;
    let mut finish = String::new();
    let mut sink = |bytes: &[u8]| {
        for &b in bytes {
            if b != b'\n' {
                line.push(b);
                continue;
            }
            let l = String::from_utf8_lossy(&line).trim().to_string();
            line.clear();
            let Some(data) = l.strip_prefix("data:").map(str::trim) else { continue };
            if data == "[DONE]" {
                return false;
            }
            let Some(v) = arcjson::parse(data) else { continue };
            if !v.get("error").is_null() {
                error = Some(v.get("error").get("message").as_str().unwrap_or("the model stopped").to_string());
                return false;
            }
            let ch = v.get("choices").at(0);
            if let Some(f) = ch.get("finish_reason").as_str() {
                finish = f.to_string();
            }
            let d = ch.get("delta");
            if let Some(_) = d.get("reasoning").as_str() {
                if !on_chunk(Chunk::Reasoning) {
                    return false;
                }
            }
            if let Some(t) = d.get("content").as_str() {
                if !t.is_empty() {
                    got_text = true;
                    if !on_chunk(Chunk::Text(t)) {
                        return false;
                    }
                }
            }
        }
        true
    };
    let r = http::stream(&req, &mut sink)?;
    if let Some(e) = error {
        return Err(e);
    }
    if !r.ok() {
        let v = arcjson::parse_bytes(&r.body);
        let msg = v.as_ref().map(|v| v.get("error").str().to_string()).filter(|s| !s.is_empty());
        return Err(match r.status {
            401 => "session expired: sign in again".to_string(),
            403 => msg.unwrap_or_else(|| "this model needs a higher plan or more credits".to_string()),
            429 => "too many requests: try again in a few seconds".to_string(),
            s => msg.unwrap_or_else(|| format!("ticly AI answered {s}")),
        });
    }
    if !got_text && finish == "length" {
        return Err("the model used all its tokens thinking: try another model".to_string());
    }
    Ok(r.header("x-ticly-model").unwrap_or(model).to_string())
}
