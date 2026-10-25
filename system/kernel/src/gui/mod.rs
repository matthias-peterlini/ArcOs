//! The graphical interface: boot splash, first-start setup and the desktop.
//!
//! When the loader gives the kernel a framebuffer of at least 800x600, the
//! console stops drawing on the screen (it keeps writing to the serial port
//! and the journal) and the screen belongs to this module:
//!
//! 1. [`boot_start`] shows the splash as soon as the screen is up and
//!    [`boot_progress`] moves its bar as the kernel starts its parts;
//! 2. [`start`] runs the interface thread: it loads icons and wallpapers from
//!    `sys:/system/gui`, then shows the setup on the first start (until it
//!    is done once) and the desktop after that.
//!
//! Drawing happens in a back buffer ([`gfx::Canvas`]); only the parts that
//! changed are copied to the framebuffer, and the pointer is drawn straight
//! on the framebuffer so that moving it redraws nothing.

pub mod gfx;
pub mod qoi;
pub mod text;

mod apps;
mod cursor;
mod desktop;
mod res;
mod settings;
mod setup;
mod splash;
mod ui;
mod wm;

use crate::console::FbInfo;
use crate::sync::IrqLock;
use gfx::{Canvas, Rect};

/// The framebuffer and the back buffer shown on it.
pub struct Display {
    fb: FbInfo,
    pub scene: Canvas,
}

impl Display {
    pub fn width(&self) -> i32 {
        self.scene.w
    }

    pub fn height(&self) -> i32 {
        self.scene.h
    }

    /// Copies a region of the back buffer to the screen.
    pub fn present(&self, r: Rect) {
        let r = r.intersect(&self.scene.bounds());
        if r.is_empty() {
            return;
        }
        let fast = self.fb.shift == (16, 8, 0);
        for y in r.y..r.bottom() {
            let src = &self.scene.px[(y * self.scene.w + r.x) as usize..(y * self.scene.w + r.right()) as usize];
            let dst = (self.fb.virt + y as u64 * self.fb.pitch as u64 + r.x as u64 * 4) as *mut u32;
            if fast {
                // SAFETY: row y, columns r.x..r.right() lie inside the mapped
                // framebuffer (r is clipped to the screen).
                unsafe { core::ptr::copy_nonoverlapping(src.as_ptr(), dst, src.len()) };
            } else {
                for (i, &p) in src.iter().enumerate() {
                    let v = self.convert(p);
                    // SAFETY: as above.
                    unsafe { dst.add(i).write_volatile(v) };
                }
            }
        }
    }

    fn convert(&self, p: u32) -> u32 {
        let (r, g, b) = ((p >> 16) & 0xFF, (p >> 8) & 0xFF, p & 0xFF);
        r << self.fb.shift.0 | g << self.fb.shift.1 | b << self.fb.shift.2
    }

    /// Writes one pixel straight to the screen (the pointer).
    fn put_screen(&self, x: i32, y: i32, p: u32) {
        if x < 0 || y < 0 || x >= self.scene.w || y >= self.scene.h {
            return;
        }
        let v = if self.fb.shift == (16, 8, 0) { p } else { self.convert(p) };
        let at = (self.fb.virt + y as u64 * self.fb.pitch as u64 + x as u64 * 4) as *mut u32;
        // SAFETY: (x, y) is on the screen.
        unsafe { at.write_volatile(v) };
    }
}

/// The display during start-up (the splash). The interface thread then
/// takes it and owns it: drawing a frame must not happen under a lock, which
/// would keep interrupts off for its whole duration.
static DISPLAY: IrqLock<Option<Display>> = IrqLock::new(None);
static ACTIVE: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);

/// Runs `f` with the start-up display (only while the splash is shown).
pub fn with_display<T>(f: impl FnOnce(&mut Display) -> T) -> Option<T> {
    DISPLAY.lock().as_mut().map(f)
}

/// Is the screen graphical (the interface is in charge of it)?
pub fn active() -> bool {
    ACTIVE.load(core::sync::atomic::Ordering::Relaxed)
}

/// Called right after the console: takes the framebuffer and shows the splash.
pub fn boot_start() {
    let Some(fb) = crate::console::framebuffer() else { return };
    let scene = Canvas::new(fb.width as i32, fb.height as i32, 0xFF00_0000);
    *DISPLAY.lock() = Some(Display { fb, scene });
    ACTIVE.store(true, core::sync::atomic::Ordering::Relaxed);
    splash::show(0);
}

/// Moves the splash's bar (0..=100).
pub fn boot_progress(percent: u32) {
    splash::show(percent);
}

/// Starts the interface thread once the system is up.
pub fn start() {
    if !active() {
        return;
    }
    crate::task::spawn("gui", run);
}

fn run() {
    // Loading the pictures is the last stretch of the bar.
    splash::show(85);
    res::load(|p| splash::show(85 + p * 15 / 100));
    splash::show(100);
    crate::task::sleep_ms(250);
    let Some(mut d) = DISPLAY.lock().take() else { return };
    if !settings::get().setup_done {
        setup::run(&mut d);
    }
    desktop::run(&mut d);
}
