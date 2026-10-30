//! Updates: the latest arcOS release on GitHub, compared with this
//! version. Checked once the network is up, and again on request from Info
//! sistema; a newer one is announced with a notification.

use crate::sync::IrqLock;
use alloc::format;
use alloc::string::String;

/// Where arcOS is released.
pub const REPO: &str = "matthias-peterlini/ArcOs";

#[derive(Clone, PartialEq, Eq)]
pub enum State {
    NotChecked,
    Checking,
    UpToDate,
    /// A newer version, as "0.7.0".
    Available(String),
    Failed(String),
}

static STATE: IrqLock<State> = IrqLock::new(State::NotChecked);

pub fn state() -> State {
    STATE.lock().clone()
}

/// "v0.7.0" or "0.7" as numbers, for comparing.
fn parse(v: &str) -> [u32; 3] {
    let mut out = [0; 3];
    for (i, part) in v.trim().trim_start_matches(['v', 'V']).split(['.', '-']).take(3).enumerate() {
        out[i] = part.parse().unwrap_or(0);
    }
    out
}

/// Whether `tag` is a newer version than `current`.
pub fn newer(tag: &str, current: &str) -> bool {
    parse(tag) > parse(current)
}

/// Asks GitHub in a thread of its own; `announce` shows a notification
/// when a newer version is found.
pub fn check(announce: bool) {
    {
        let mut s = STATE.lock();
        if *s == State::Checking {
            return;
        }
        *s = State::Checking;
    }
    let spawned = crate::task::spawn("update", move || {
        let result = match crate::store::github_release(REPO) {
            Ok((tag, _)) if newer(&tag, crate::VERSION) => State::Available(String::from(tag.trim_start_matches(['v', 'V']))),
            Ok(_) => State::UpToDate,
            Err(e) => State::Failed(e),
        };
        if let (true, State::Available(v)) = (announce, &result) {
            crate::gui::notify("Aggiornamento disponibile", &format!("arcOS {v} è pronto: trovalo in Info sistema."), "about");
        }
        *STATE.lock() = result;
    });
    if spawned.is_none() {
        *STATE.lock() = State::Failed(String::from("no thread"));
    }
}

/// Waits (up to a minute) for the network, then checks once.
pub fn check_when_online() {
    crate::task::spawn("update-wait", || {
        for _ in 0..60 {
            if crate::net::online() {
                check(true);
                return;
            }
            crate::task::sleep_ms(1000);
        }
    });
}
