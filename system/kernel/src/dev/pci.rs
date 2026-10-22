//! PCI configuration space (mechanism #1, ports 0xCF8/0xCFC) and bus scan.

use crate::arch::cpu::{inl, outl};
use crate::sync::IrqLock;
use alloc::vec::Vec;
use arcdk::{DeviceInfo, Resource, BUS_PCI, IRQ_NONE, RES_64BIT, RES_IO, RES_MEMORY, RES_PREFETCH};

static CONFIG: IrqLock<()> = IrqLock::new(());

fn address(loc: u32, off: u16) -> u32 {
    0x8000_0000 | loc << 8 | (off as u32 & 0xFC)
}

/// `loc` is bus << 8 | slot << 3 | function.
pub fn read32(loc: u32, off: u16) -> u32 {
    let _g = CONFIG.lock();
    outl(0xCF8, address(loc, off));
    inl(0xCFC)
}

pub fn write32(loc: u32, off: u16, v: u32) {
    let _g = CONFIG.lock();
    outl(0xCF8, address(loc, off));
    outl(0xCFC, v);
}

pub fn read(loc: u32, off: u16, width: u8) -> u32 {
    let v = read32(loc, off & !3);
    let shift = (off & 3) * 8;
    match width {
        1 => (v >> shift) & 0xFF,
        2 => (v >> shift) & 0xFFFF,
        _ => v,
    }
}

pub fn write(loc: u32, off: u16, width: u8, value: u32) {
    match width {
        4 => write32(loc, off & !3, value),
        1 | 2 => {
            let aligned = off & !3;
            let shift = (off & 3) * 8;
            let mask = if width == 1 { 0xFFu32 } else { 0xFFFF } << shift;
            let old = read32(loc, aligned);
            write32(loc, aligned, (old & !mask) | ((value << shift) & mask));
        }
        _ => {}
    }
}

/// Size and decode the six BARs of a type-0 header.
fn read_bars(loc: u32) -> [Resource; 6] {
    let mut out = [Resource::default(); 6];
    let cmd = read(loc, 0x04, 2);
    // Stop decoding while the BARs hold sizing patterns.
    write(loc, 0x04, 2, cmd & !0x3);
    let mut i = 0;
    while i < 6 {
        let off = 0x10 + i as u16 * 4;
        let orig = read32(loc, off);
        write32(loc, off, 0xFFFF_FFFF);
        let probe = read32(loc, off);
        write32(loc, off, orig);
        if orig & 1 == 1 {
            let mask = probe & 0xFFFF_FFFC;
            if mask != 0 {
                let size = (!mask).wrapping_add(1) & 0xFFFF;
                out[i] = Resource { kind: RES_IO, flags: 0, base: (orig & 0xFFFF_FFFC) as u64, size: size as u64 };
            }
            i += 1;
            continue;
        }
        let is64 = (orig >> 1) & 3 == 2;
        let prefetch = if orig & 8 != 0 { RES_PREFETCH } else { 0 };
        let mut base = (orig & 0xFFFF_FFF0) as u64;
        let mut mask = (probe & 0xFFFF_FFF0) as u64;
        if is64 && i < 5 {
            let off_hi = off + 4;
            let orig_hi = read32(loc, off_hi);
            write32(loc, off_hi, 0xFFFF_FFFF);
            let probe_hi = read32(loc, off_hi);
            write32(loc, off_hi, orig_hi);
            base |= (orig_hi as u64) << 32;
            mask |= (probe_hi as u64) << 32;
        } else if mask != 0 {
            mask |= 0xFFFF_FFFF_0000_0000;
        }
        if mask != 0 {
            out[i] = Resource { kind: RES_MEMORY, flags: prefetch | if is64 { RES_64BIT } else { 0 }, base, size: (!mask).wrapping_add(1) };
        }
        i += if is64 { 2 } else { 1 };
    }
    write(loc, 0x04, 2, cmd);
    out
}

fn function_info(loc: u32) -> Option<DeviceInfo> {
    let id = read32(loc, 0x00);
    if id & 0xFFFF == 0xFFFF {
        return None;
    }
    let class = read32(loc, 0x08);
    let header = read(loc, 0x0E, 1) & 0x7F;
    let mut info = DeviceInfo::empty();
    info.bus = BUS_PCI;
    info.vendor = id as u16;
    info.device = (id >> 16) as u16;
    info.revision = class as u8;
    info.prog_if = (class >> 8) as u8;
    info.subclass = (class >> 16) as u8;
    info.class = (class >> 24) as u8;
    info.location = loc;
    if header == 0 {
        let sub = read32(loc, 0x2C);
        info.subsystem_vendor = sub as u16;
        info.subsystem = (sub >> 16) as u16;
        info.resources = read_bars(loc);
        let line = read(loc, 0x3C, 1) as u8;
        let pin = read(loc, 0x3D, 1);
        info.irq = if pin != 0 && line < 16 { line } else { IRQ_NONE };
    }
    Some(info)
}

/// Every PCI function on every bus.
pub fn scan() -> Vec<DeviceInfo> {
    let mut out = Vec::new();
    for bus in 0..256u32 {
        for slot in 0..32u32 {
            let loc = bus << 8 | slot << 3;
            let Some(first) = function_info(loc) else { continue };
            let multi = read(loc, 0x0E, 1) & 0x80 != 0;
            out.push(first);
            if multi {
                for func in 1..8 {
                    if let Some(f) = function_info(loc | func) {
                        out.push(f);
                    }
                }
            }
        }
    }
    out
}

/// Human-readable name of a PCI class.
pub fn class_name(class: u8, subclass: u8, prog_if: u8) -> &'static str {
    match (class, subclass, prog_if) {
        (0x01, 0x01, _) => "IDE controller",
        (0x01, 0x06, 0x01) => "SATA controller (AHCI)",
        (0x01, 0x06, _) => "SATA controller",
        (0x01, 0x08, 0x02) => "NVMe controller",
        (0x01, 0x00, _) => "SCSI controller",
        (0x01, 0x80, _) => "storage controller",
        (0x01, _, _) => "storage controller",
        (0x02, 0x00, _) => "Ethernet controller",
        (0x02, 0x80, _) => "network controller",
        (0x02, _, _) => "network controller",
        (0x03, 0x00, _) => "VGA display",
        (0x03, _, _) => "display controller",
        (0x04, 0x01, _) => "audio device",
        (0x04, 0x03, _) => "HD audio controller",
        (0x04, _, _) => "multimedia device",
        (0x05, _, _) => "memory controller",
        (0x06, 0x00, _) => "host bridge",
        (0x06, 0x01, _) => "ISA bridge",
        (0x06, 0x04, _) => "PCI bridge",
        (0x06, _, _) => "bridge",
        (0x07, 0x00, _) => "serial controller",
        (0x07, _, _) => "communication controller",
        (0x08, _, _) => "system peripheral",
        (0x09, _, _) => "input controller",
        (0x0C, 0x03, 0x00) => "USB 1.1 controller (UHCI)",
        (0x0C, 0x03, 0x10) => "USB 1.1 controller (OHCI)",
        (0x0C, 0x03, 0x20) => "USB 2.0 controller (EHCI)",
        (0x0C, 0x03, 0x30) => "USB 3 controller (xHCI)",
        (0x0C, 0x03, _) => "USB controller",
        (0x0C, 0x05, _) => "SMBus controller",
        (0x0C, _, _) => "serial bus controller",
        (0x0D, _, _) => "wireless controller",
        _ => "device",
    }
}

pub fn vendor_name(vendor: u16) -> Option<&'static str> {
    Some(match vendor {
        0x8086 => "Intel",
        0x1022 => "AMD",
        0x10DE => "NVIDIA",
        0x1002 => "AMD/ATI",
        0x10EC => "Realtek",
        0x14E4 => "Broadcom",
        0x1AF4 => "Red Hat (virtio)",
        0x1B36 => "Red Hat (QEMU)",
        0x1234 => "QEMU/Bochs",
        0x15AD => "VMware",
        0x80EE => "VirtualBox",
        0x144D => "Samsung",
        0x1B4B => "Marvell",
        0x168C => "Qualcomm Atheros",
        0x1D6A => "Aquantia",
        0x1AE0 => "Google",
        _ => return None,
    })
}
