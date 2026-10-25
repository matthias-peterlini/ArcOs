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
    /// Screen resolution chosen by the user (needs a display driver).
    pub resolution: Option<(u32, u32)>,
    /// Sound volume, 0 to 100.
    pub volume: u32,
    /// Play a sound when the desktop appears.
    pub startup_sound: bool,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            name: String::new(),
            layout: Layout::It,
            wallpaper: String::from("dolomiti"),
            setup_done: false,
            resolution: None,
            volume: 70,
            startup_sound: true,
        }
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
            "resolution" => {
                s.resolution = v.split_once('x').and_then(|(w, h)| Some((w.parse().ok()?, h.parse().ok()?)));
            }
            "volume" => s.volume = v.parse::<u32>().unwrap_or(70).min(100),
            "startup-sound" => s.startup_sound = v != "off",
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
        crate::dev::audio::set_volume(s.volume);
        *cur = Some(s);
    }
    cur.clone().unwrap()
}

/// Changes the settings, applies them and writes them to disk.
pub fn update(f: impl FnOnce(&mut Settings)) {
    let mut s = get();
    f(&mut s);
    input::set_layout(s.layout);
    crate::dev::audio::set_volume(s.volume);
    let mut text = format!(
        "name={}\nlayout={}\nwallpaper={}\nsetup={}\nvolume={}\nstartup-sound={}\n",
        s.name,
        if s.layout == Layout::Us { "us" } else { "it" },
        s.wallpaper,
        if s.setup_done { "done" } else { "no" },
        s.volume,
        if s.startup_sound { "on" } else { "off" }
    );
    if let Some((w, h)) = s.resolution {
        text.push_str(&format!("resolution={w}x{h}\n"));
    }
    if let Ok(p) = fs::abs(FILE) {
        if let Err(e) = fs::write_file(&p, text.as_bytes()) {
            crate::warn!("gui", "cannot save the settings: {e}");
        }
    }
    *CURRENT.lock() = Some(s);
}
