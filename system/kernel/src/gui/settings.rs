//! The user's settings, kept in `sys:/system/settings` as `key=value` lines.

use crate::dev::input;
use crate::dev::keymap::Layout;
use crate::fs;
use crate::sync::IrqLock;
use alloc::format;
use alloc::string::String;

const FILE: &str = "sys:/system/settings";

#[derive(Clone)]
pub struct Settings {
    pub name: String,
    pub layout: Layout,
    pub wallpaper: String,
    pub setup_done: bool,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings { name: String::new(), layout: Layout::It, wallpaper: String::from("dolomiti"), setup_done: false }
    }
}

static CURRENT: IrqLock<Option<Settings>> = IrqLock::new(None);

fn load() -> Settings {
    let mut s = Settings::default();
    let Ok(bytes) = fs::abs(FILE).and_then(|p| fs::read_file(&p)) else { return s };
    for line in String::from_utf8_lossy(&bytes).lines() {
        let Some((k, v)) = line.split_once('=') else { continue };
        let v = v.trim();
        match k.trim() {
            "name" => s.name = String::from(v),
            "layout" => s.layout = if v == "us" { Layout::Us } else { Layout::It },
            "wallpaper" => s.wallpaper = String::from(v),
            "setup" => s.setup_done = v == "done",
            _ => {}
        }
    }
    s
}

/// The current settings (read from disk the first time).
pub fn get() -> Settings {
    let mut cur = CURRENT.lock();
    if cur.is_none() {
        let s = load();
        if s.setup_done {
            input::set_layout(s.layout);
        }
        *cur = Some(s);
    }
    cur.clone().unwrap()
}

/// Changes the settings, applies them and writes them to disk.
pub fn update(f: impl FnOnce(&mut Settings)) {
    let mut s = get();
    f(&mut s);
    input::set_layout(s.layout);
    let text = format!(
        "name={}\nlayout={}\nwallpaper={}\nsetup={}\n",
        s.name,
        if s.layout == Layout::Us { "us" } else { "it" },
        s.wallpaper,
        if s.setup_done { "done" } else { "no" }
    );
    if let Ok(p) = fs::abs(FILE) {
        if let Err(e) = fs::write_file(&p, text.as_bytes()) {
            crate::warn!("gui", "cannot save the settings: {e}");
        }
    }
    *CURRENT.lock() = Some(s);
}
