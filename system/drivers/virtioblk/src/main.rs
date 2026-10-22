//! virtio block driver (legacy PCI interface).
//!
//! One virtqueue, one request at a time: header, data (bounce buffer) and
//! status descriptors chained together, completion polled on the used ring.

#![no_std]
#![no_main]

extern crate alloc;

use alloc::boxed::Box;
use arcdrv::{port, BlockDesc, DeviceInfo, Dma, Mutex, Status, BLOCK_READONLY, E_INVAL, E_IO, E_NODEV, E_NOMEM, E_TIMEOUT, OK, RES_IO};
use core::ffi::c_void;
use core::sync::atomic::{fence, Ordering};

arcdrv::driver! {
    name: "virtioblk",
    version: "1.0.0",
    probe: probe,
}

// Legacy virtio PCI registers (I/O BAR 0).
const DEVICE_FEATURES: u16 = 0x00;
const GUEST_FEATURES: u16 = 0x04;
const QUEUE_PFN: u16 = 0x08;
const QUEUE_SIZE: u16 = 0x0C;
const QUEUE_SELECT: u16 = 0x0E;
const QUEUE_NOTIFY: u16 = 0x10;
const DEVICE_STATUS: u16 = 0x12;
const CONFIG: u16 = 0x14;

const STATUS_ACK: u8 = 1;
const STATUS_DRIVER: u8 = 2;
const STATUS_DRIVER_OK: u8 = 4;
const STATUS_FAILED: u8 = 0x80;

const FEATURE_RO: u32 = 1 << 5;
const FEATURE_FLUSH: u32 = 1 << 9;

const DESC_NEXT: u16 = 1;
const DESC_WRITE: u16 = 2;

const T_IN: u32 = 0;
const T_OUT: u32 = 1;
const T_FLUSH: u32 = 4;

const MAX_SECTORS: u32 = 128;

struct Device {
    io: u16,
    size: u16,
    ring: Dma,
    avail: usize,
    used: usize,
    last_used: u16,
    req: Dma,
    bounce: Dma,
    flush: bool,
}

struct Disk {
    dev: Mutex<Device>,
    sectors: u64,
}

impl Device {
    fn desc(&self, i: u16, addr: u64, len: u32, flags: u16, next: u16) {
        let at = i as usize * 16;
        self.ring.write::<u64>(at, addr);
        self.ring.write::<u32>(at + 8, len);
        self.ring.write::<u16>(at + 12, flags);
        self.ring.write::<u16>(at + 14, next);
    }

    fn request(&mut self, kind: u32, sector: u64, bytes: usize) -> Status {
        // Header: type, reserved, sector. Status byte after it.
        self.req.write::<u32>(0, kind);
        self.req.write::<u32>(4, 0);
        self.req.write::<u64>(8, sector);
        self.req.write::<u8>(16, 0xFF);
        let header = self.req.phys();
        if bytes > 0 {
            self.desc(0, header, 16, DESC_NEXT, 1);
            let data_flags = DESC_NEXT | if kind == T_IN { DESC_WRITE } else { 0 };
            self.desc(1, self.bounce.phys(), bytes as u32, data_flags, 2);
            self.desc(2, header + 16, 1, DESC_WRITE, 0);
        } else {
            self.desc(0, header, 16, DESC_NEXT, 2);
            self.desc(2, header + 16, 1, DESC_WRITE, 0);
        }
        // Publish descriptor 0 in the available ring.
        let idx = self.ring.read::<u16>(self.avail + 2);
        self.ring.write::<u16>(self.avail + 4 + (idx % self.size) as usize * 2, 0);
        fence(Ordering::SeqCst);
        self.ring.write::<u16>(self.avail + 2, idx.wrapping_add(1));
        fence(Ordering::SeqCst);
        port::outw(self.io + QUEUE_NOTIFY, 0);
        let target = self.last_used.wrapping_add(1);
        if !arcdrv::wait_until(5000, || self.ring.read::<u16>(self.used + 2) == target) {
            return E_TIMEOUT;
        }
        self.last_used = target;
        // Reading the ISR register acknowledges the interrupt line.
        port::inb(self.io + 0x13);
        if self.req.read::<u8>(16) == 0 {
            OK
        } else {
            E_IO
        }
    }
}

impl Disk {
    fn transfer(&self, lba: u64, count: u32, buf: *mut u8, write: bool) -> Status {
        if count == 0 || lba + count as u64 > self.sectors {
            return E_INVAL;
        }
        let mut d = self.dev.lock();
        let mut done = 0;
        while done < count {
            let n = (count - done).min(MAX_SECTORS);
            let bytes = n as usize * 512;
            // SAFETY: the kernel passes a buffer of `count` sectors.
            let chunk = unsafe { core::slice::from_raw_parts_mut(buf.add(done as usize * 512), bytes) };
            if write {
                d.bounce.as_mut_slice()[..bytes].copy_from_slice(chunk);
            }
            let st = d.request(if write { T_OUT } else { T_IN }, lba + done as u64, bytes);
            if st != OK {
                return st;
            }
            if !write {
                chunk.copy_from_slice(&d.bounce.as_slice()[..bytes]);
            }
            done += n;
        }
        OK
    }
}

unsafe extern "C" fn read(ctx: *mut c_void, lba: u64, count: u32, buf: *mut u8) -> Status {
    // SAFETY: ctx is the Disk registered with this descriptor.
    unsafe { &*(ctx as *const Disk) }.transfer(lba, count, buf, false)
}

unsafe extern "C" fn write(ctx: *mut c_void, lba: u64, count: u32, buf: *mut u8) -> Status {
    // SAFETY: as in read.
    unsafe { &*(ctx as *const Disk) }.transfer(lba, count, buf, true)
}

unsafe extern "C" fn flush(ctx: *mut c_void) -> Status {
    // SAFETY: as in read.
    let disk = unsafe { &*(ctx as *const Disk) };
    let mut d = disk.dev.lock();
    if !d.flush {
        return OK;
    }
    d.request(T_FLUSH, 0, 0)
}

fn probe(dev: &DeviceInfo) -> Status {
    if dev.vendor != 0x1AF4 || dev.device != 0x1001 {
        return E_NODEV;
    }
    let Some(io) = (dev.resources[0].kind == RES_IO).then_some(dev.resources[0].base as u16) else {
        arcdrv::warn!("no legacy I/O window: modern-only virtio is not supported yet");
        return E_IO;
    };
    arcdrv::Pci(dev.id).enable(true, false, true);
    port::outb(io + DEVICE_STATUS, 0);
    port::outb(io + DEVICE_STATUS, STATUS_ACK | STATUS_DRIVER);
    let features = port::inl(io + DEVICE_FEATURES);
    let accepted = features & (FEATURE_RO | FEATURE_FLUSH);
    port::outl(io + GUEST_FEATURES, accepted);

    port::outw(io + QUEUE_SELECT, 0);
    let size = port::inw(io + QUEUE_SIZE);
    if size == 0 || size > 1024 {
        port::outb(io + DEVICE_STATUS, STATUS_FAILED);
        return E_IO;
    }
    // Legacy layout: descriptors, available ring, then the used ring on the
    // next 4 KiB boundary.
    let avail = size as usize * 16;
    let used = (avail + 6 + 2 * size as usize).next_multiple_of(4096);
    let total = used + 6 + 8 * size as usize;
    let (Some(ring), Some(req), Some(bounce)) = (Dma::new(total), Dma::new(64), Dma::new(MAX_SECTORS as usize * 512)) else {
        return E_NOMEM;
    };
    port::outl(io + QUEUE_PFN, (ring.phys() >> 12) as u32);
    port::outb(io + DEVICE_STATUS, STATUS_ACK | STATUS_DRIVER | STATUS_DRIVER_OK);

    let sectors = port::inl(io + CONFIG) as u64 | (port::inl(io + CONFIG + 4) as u64) << 32;
    let device = Device { io, size, ring, avail, used, last_used: 0, req, bounce, flush: accepted & FEATURE_FLUSH != 0 };
    let disk: &'static Disk = Box::leak(Box::new(Disk { dev: Mutex::new(device), sectors }));
    let desc: &'static BlockDesc = Box::leak(Box::new(BlockDesc {
        ctx: disk as *const Disk as *mut c_void,
        sector_size: 512,
        flags: if accepted & FEATURE_RO != 0 { BLOCK_READONLY } else { 0 },
        sector_count: sectors,
        read,
        write,
        flush: Some(flush),
        model: arcdrv::model_name("virtio disk"),
    }));
    let n = arcdrv::block_register(dev.id, desc);
    if n < 0 {
        n
    } else {
        OK
    }
}
