//! virtio GPU display driver (2D).
//!
//! The screen is a resource on the host backed by a framebuffer in guest
//! memory: arcOS draws into that memory, and every change is copied to the
//! host and shown with two commands (transfer, flush). Commands go one at a
//! time through the control queue and are polled.

#![no_std]
#![no_main]

extern crate alloc;

use alloc::boxed::Box;
use alloc::vec::Vec;
use arcdrv::{DeviceInfo, DisplayDesc, DisplayMode, Dma, Framebuffer, Mutex, Status, E_INVAL, E_IO, E_NODEV, E_NOMEM, OK};
use arcvirtio::{Queue, Virtio, DESC_NEXT, DESC_WRITE};
use core::ffi::c_void;

arcdrv::driver! {
    name: "virtiogpu",
    version: "1.0.0",
    probe: probe,
}

const GET_DISPLAY_INFO: u32 = 0x0100;
const RESOURCE_CREATE_2D: u32 = 0x0101;
const RESOURCE_UNREF: u32 = 0x0102;
const SET_SCANOUT: u32 = 0x0103;
const RESOURCE_FLUSH: u32 = 0x0104;
const TRANSFER_TO_HOST_2D: u32 = 0x0105;
const RESOURCE_ATTACH_BACKING: u32 = 0x0106;
const RESP_OK_NODATA: u32 = 0x1100;
const RESP_OK_DISPLAY_INFO: u32 = 0x1101;

/// Blue, green, red, unused: 0x00RRGGBB in a little-endian word.
const FORMAT_B8G8R8X8: u32 = 2;
const HEADER: usize = 24;
const MAX_SIZE: (u32, u32) = (3840, 2160);

const MODES: &[(u32, u32)] = &[
    (800, 600),
    (1024, 768),
    (1280, 720),
    (1280, 800),
    (1280, 1024),
    (1366, 768),
    (1440, 900),
    (1600, 900),
    (1680, 1050),
    (1920, 1080),
    (1920, 1200),
    (2560, 1440),
    (2560, 1600),
];

struct Screen {
    resource: u32,
    width: u32,
    height: u32,
    memory: Option<Dma>,
}

struct Gpu {
    vio: Virtio,
    /// The control queue, its command page and the current screen.
    state: Mutex<(Queue, Dma, Screen)>,
    /// The host's preferred size for the first output.
    preferred: (u32, u32),
}

/// Writes little-endian words into a command buffer.
struct Cmd<'a> {
    buf: &'a mut [u8],
    len: usize,
}

impl Cmd<'_> {
    fn u32(&mut self, v: u32) -> &mut Self {
        self.buf[self.len..self.len + 4].copy_from_slice(&v.to_le_bytes());
        self.len += 4;
        self
    }

    fn u64(&mut self, v: u64) -> &mut Self {
        self.buf[self.len..self.len + 8].copy_from_slice(&v.to_le_bytes());
        self.len += 8;
        self
    }
}

impl Gpu {
    /// Sends a command built by `build` (after its header) and waits for
    /// the answer. Returns the answer's type; the answer stays in the page
    /// at offset 2048.
    fn command(&self, q: &mut Queue, page: &Dma, kind: u32, build: impl FnOnce(&mut Cmd<'_>)) -> u32 {
        let buf = page.as_mut_slice();
        buf[..4096].fill(0);
        let mut c = Cmd { buf: &mut buf[..2048], len: 0 };
        c.u32(kind).u32(0).u64(0).u32(0).u32(0);
        build(&mut c);
        let len = c.len;
        q.desc(0, page.phys(), len as u32, DESC_NEXT, 1);
        q.desc(1, page.phys() + 2048, 2048, DESC_WRITE, 0);
        q.publish(0);
        self.vio.notify(q);
        if !arcdrv::wait_until(1000, || q.take_used().is_some()) {
            return 0;
        }
        page.read::<u32>(2048)
    }

    fn ok(&self, q: &mut Queue, page: &Dma, kind: u32, build: impl FnOnce(&mut Cmd<'_>)) -> bool {
        self.command(q, page, kind, build) == RESP_OK_NODATA
    }
}

unsafe extern "C" fn modes(ctx: *mut c_void, out: *mut DisplayMode, max: usize) -> usize {
    // SAFETY: ctx is the leaked Gpu; out holds max modes.
    let g = unsafe { &*(ctx as *const Gpu) };
    let mut list: Vec<(u32, u32)> = MODES.to_vec();
    if g.preferred.0 >= 640 && !list.contains(&g.preferred) {
        list.push(g.preferred);
    }
    let n = list.len().min(max);
    for (i, &(width, height)) in list.iter().take(n).enumerate() {
        // SAFETY: i < max.
        unsafe { out.add(i).write(DisplayMode { width, height }) };
    }
    n
}

unsafe extern "C" fn set_mode(ctx: *mut c_void, mode: DisplayMode, fb: *mut Framebuffer) -> Status {
    // SAFETY: ctx is the leaked Gpu; fb is valid for the call.
    let g = unsafe { &*(ctx as *const Gpu) };
    let (w, h) = (mode.width, mode.height);
    if w < 640 || h < 480 || w > MAX_SIZE.0 || h > MAX_SIZE.1 {
        return E_INVAL;
    }
    let Some(memory) = Dma::new(w as usize * h as usize * 4) else { return E_NOMEM };
    let mut st = g.state.lock();
    let (q, page, screen) = &mut *st;
    let id = screen.resource + 1;
    let created = g.ok(q, page, RESOURCE_CREATE_2D, |c| {
        c.u32(id).u32(FORMAT_B8G8R8X8).u32(w).u32(h);
    }) && g.ok(q, page, RESOURCE_ATTACH_BACKING, |c| {
        c.u32(id).u32(1).u64(memory.phys()).u32(memory.len() as u32).u32(0);
    }) && g.ok(q, page, SET_SCANOUT, |c| {
        c.u32(0).u32(0).u32(w).u32(h).u32(0).u32(id);
    });
    if !created {
        return E_IO;
    }
    // The old screen goes once the new one shows.
    if screen.resource != 0 {
        let old = screen.resource;
        g.ok(q, page, RESOURCE_UNREF, |c| {
            c.u32(old).u32(0);
        });
    }
    // SAFETY: as above.
    unsafe {
        *fb = Framebuffer {
            phys: memory.phys(),
            virt: memory.as_ptr() as u64,
            width: w,
            height: h,
            pitch: w * 4,
            red: 16,
            green: 8,
            blue: 0,
            _pad: 0,
        };
    }
    *screen = Screen { resource: id, width: w, height: h, memory: Some(memory) };
    OK
}

unsafe extern "C" fn flush(ctx: *mut c_void, x: u32, y: u32, w: u32, h: u32) {
    // SAFETY: ctx is the leaked Gpu.
    let g = unsafe { &*(ctx as *const Gpu) };
    let mut st = g.state.lock();
    let (q, page, screen) = &mut *st;
    if screen.memory.is_none() || x >= screen.width || y >= screen.height {
        return;
    }
    let (w, h) = (w.min(screen.width - x), h.min(screen.height - y));
    let (id, pitch) = (screen.resource, screen.width as u64 * 4);
    g.ok(q, page, TRANSFER_TO_HOST_2D, |c| {
        c.u32(x).u32(y).u32(w).u32(h).u64(y as u64 * pitch + x as u64 * 4).u32(id).u32(0);
    });
    g.ok(q, page, RESOURCE_FLUSH, |c| {
        c.u32(x).u32(y).u32(w).u32(h).u32(id).u32(0);
    });
}

fn probe(dev: &DeviceInfo) -> Status {
    let Some(vio) = Virtio::open(dev) else {
        arcdrv::warn!("no usable virtio registers");
        return E_NODEV;
    };
    if vio.negotiate(0).is_none() {
        return E_IO;
    }
    let (Some(q), Some(page)) = (vio.queue(0, 16), Dma::new(4096)) else {
        vio.set_status(arcvirtio::STATUS_FAILED);
        return E_IO;
    };
    q.quiet();
    vio.ready();
    let mut g = Gpu { vio, state: Mutex::new((q, page, Screen { resource: 0, width: 0, height: 0, memory: None })), preferred: (0, 0) };
    let preferred = {
        let mut st = g.state.lock();
        let (q, page, _) = &mut *st;
        // The first of 16 outputs: rectangle, enabled.
        let at = 2048 + HEADER;
        if g.command(q, page, GET_DISPLAY_INFO, |_| {}) == RESP_OK_DISPLAY_INFO && page.read::<u32>(at + 16) != 0 {
            (page.read::<u32>(at + 8), page.read::<u32>(at + 12))
        } else {
            (0, 0)
        }
    };
    g.preferred = preferred;
    arcdrv::info!("output 0: {}x{} preferred", g.preferred.0, g.preferred.1);
    let g: &'static Gpu = Box::leak(Box::new(g));
    let desc: &'static DisplayDesc = Box::leak(Box::new(DisplayDesc {
        ctx: g as *const Gpu as *mut c_void,
        modes,
        set_mode,
        flush: Some(flush),
        name: arcdrv::model_name("virtio GPU"),
    }));
    let n = arcdrv::display_register(dev.id, desc);
    if n < 0 {
        return n;
    }
    OK
}
