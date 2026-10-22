//! Intel PRO/1000 (e1000) Ethernet driver.
//!
//! Descriptor rings for receive and transmit, 2 KiB buffers, receive
//! interrupts on the legacy line (with a polling thread as a fallback).

#![no_std]
#![no_main]

extern crate alloc;

use alloc::boxed::Box;
use arcdrv::{DeviceInfo, Dma, IrqLock, Mmio, NetDesc, Status, E_INVAL, E_IO, E_NODEV, E_NOMEM, IRQ_NONE, OK, RES_MEMORY};
use core::ffi::c_void;
use core::sync::atomic::{AtomicI32, Ordering};

arcdrv::driver! {
    name: "e1000",
    version: "1.0.0",
    probe: probe,
}

const CTRL: usize = 0x0000;
const STATUS: usize = 0x0008;
const EERD: usize = 0x0014;
const ICR: usize = 0x00C0;
const IMS: usize = 0x00D0;
const IMC: usize = 0x00D8;
const RCTL: usize = 0x0100;
const TCTL: usize = 0x0400;
const TIPG: usize = 0x0410;
const RDBAL: usize = 0x2800;
const RDBAH: usize = 0x2804;
const RDLEN: usize = 0x2808;
const RDH: usize = 0x2810;
const RDT: usize = 0x2818;
const TDBAL: usize = 0x3800;
const TDBAH: usize = 0x3804;
const TDLEN: usize = 0x3808;
const TDH: usize = 0x3810;
const TDT: usize = 0x3818;
const MTA: usize = 0x5200;
const RAL: usize = 0x5400;
const RAH: usize = 0x5404;

const RX_COUNT: usize = 32;
const TX_COUNT: usize = 32;
const BUF: usize = 2048;

struct Nic {
    regs: Mmio,
    rx_ring: Dma,
    rx_bufs: Dma,
    rx_next: usize,
    tx_ring: Dma,
    tx_bufs: Dma,
    tx_next: usize,
}

static NIC: IrqLock<Option<Nic>> = IrqLock::new(None);
static IFACE: AtomicI32 = AtomicI32::new(-1);

impl Nic {
    /// Hands every completed receive descriptor to the kernel.
    fn receive(&mut self) {
        let iface = IFACE.load(Ordering::Relaxed);
        loop {
            let at = self.rx_next * 16;
            let status = self.rx_ring.read::<u8>(at + 12);
            if status & 1 == 0 {
                break; // descriptor not done
            }
            let len = self.rx_ring.read::<u16>(at + 8) as usize;
            if status & 2 != 0 && iface >= 0 && len <= BUF {
                let frame = &self.rx_bufs.as_slice()[self.rx_next * BUF..self.rx_next * BUF + len];
                arcdrv::net_receive(iface as u32, frame);
            }
            self.rx_ring.write::<u8>(at + 12, 0);
            self.regs.write32(RDT, self.rx_next as u32);
            self.rx_next = (self.rx_next + 1) % RX_COUNT;
        }
    }

    fn send(&mut self, frame: &[u8]) -> Status {
        if frame.len() > BUF || frame.is_empty() {
            return E_INVAL;
        }
        let i = self.tx_next;
        let at = i * 16;
        // Wait for the descriptor to be free (done or never used).
        if !arcdrv::wait_until(100, || {
            let st = self.tx_ring.read::<u8>(at + 12);
            st & 1 != 0 || self.tx_ring.read::<u16>(at + 8) == 0
        }) {
            return E_IO;
        }
        self.tx_bufs.as_mut_slice()[i * BUF..i * BUF + frame.len()].copy_from_slice(frame);
        self.tx_ring.write::<u64>(at, self.tx_bufs.phys() + (i * BUF) as u64);
        self.tx_ring.write::<u16>(at + 8, frame.len() as u16);
        self.tx_ring.write::<u8>(at + 11, 0b1011); // EOP | IFCS | RS
        self.tx_ring.write::<u8>(at + 12, 0);
        self.tx_next = (i + 1) % TX_COUNT;
        self.regs.write32(TDT, self.tx_next as u32);
        OK
    }
}

unsafe extern "C" fn send(_: *mut c_void, frame: *const u8, len: usize) -> Status {
    // SAFETY: the kernel passes a valid frame.
    let frame = unsafe { core::slice::from_raw_parts(frame, len) };
    match NIC.lock().as_mut() {
        Some(nic) => nic.send(frame),
        None => E_IO,
    }
}

unsafe extern "C" fn irq(_: *mut c_void) {
    if let Some(nic) = NIC.lock().as_mut() {
        let cause = nic.regs.read32(ICR); // reading clears it
        if cause != 0 {
            nic.receive();
        }
    }
}

unsafe extern "C" fn poller(_: *mut c_void) {
    loop {
        if let Some(nic) = NIC.lock().as_mut() {
            nic.receive();
        }
        arcdrv::sleep_ms(5);
    }
}

fn read_mac(regs: &Mmio) -> [u8; 6] {
    let lo = regs.read32(RAL);
    let hi = regs.read32(RAH);
    if hi & (1 << 31) != 0 && (lo != 0 || hi & 0xFFFF != 0) {
        let l = lo.to_le_bytes();
        let h = hi.to_le_bytes();
        return [l[0], l[1], l[2], l[3], h[0], h[1]];
    }
    // Not in the receive address registers: read the EEPROM.
    let mut mac = [0u8; 6];
    for word in 0..3u32 {
        regs.write32(EERD, 1 | word << 8);
        let mut v = 0;
        arcdrv::wait_until(10, || {
            v = regs.read32(EERD);
            v & (1 << 4) != 0
        });
        let data = (v >> 16) as u16;
        mac[word as usize * 2] = data as u8;
        mac[word as usize * 2 + 1] = (data >> 8) as u8;
    }
    mac
}

fn probe(dev: &DeviceInfo) -> Status {
    if dev.vendor != 0x8086 {
        return E_NODEV;
    }
    let bar = dev.resources[0];
    if bar.kind != RES_MEMORY || bar.base == 0 {
        return E_IO;
    }
    arcdrv::Pci(dev.id).enable(false, true, true);
    let Some(regs) = Mmio::map(bar.base, 0x20000) else { return E_NOMEM };

    regs.write32(IMC, u32::MAX);
    regs.write32(CTRL, regs.read32(CTRL) | (1 << 26)); // reset
    arcdrv::sleep_ms(10);
    arcdrv::wait_until(100, || regs.read32(CTRL) & (1 << 26) == 0);
    regs.write32(IMC, u32::MAX);
    regs.write32(CTRL, (regs.read32(CTRL) | (1 << 6) | (1 << 5)) & !(1 << 3)); // link up, auto speed

    let mac = read_mac(&regs);
    for i in 0..128 {
        regs.write32(MTA + i * 4, 0);
    }

    let (Some(rx_ring), Some(rx_bufs), Some(tx_ring), Some(tx_bufs)) =
        (Dma::new(RX_COUNT * 16), Dma::new(RX_COUNT * BUF), Dma::new(TX_COUNT * 16), Dma::new(TX_COUNT * BUF))
    else {
        return E_NOMEM;
    };
    for i in 0..RX_COUNT {
        rx_ring.write::<u64>(i * 16, rx_bufs.phys() + (i * BUF) as u64);
    }
    regs.write32(RDBAL, rx_ring.phys() as u32);
    regs.write32(RDBAH, (rx_ring.phys() >> 32) as u32);
    regs.write32(RDLEN, (RX_COUNT * 16) as u32);
    regs.write32(RDH, 0);
    regs.write32(RDT, (RX_COUNT - 1) as u32);
    // Enable, accept broadcast, 2 KiB buffers, strip the CRC.
    regs.write32(RCTL, (1 << 1) | (1 << 15) | (1 << 26));

    regs.write32(TDBAL, tx_ring.phys() as u32);
    regs.write32(TDBAH, (tx_ring.phys() >> 32) as u32);
    regs.write32(TDLEN, (TX_COUNT * 16) as u32);
    regs.write32(TDH, 0);
    regs.write32(TDT, 0);
    regs.write32(TCTL, (1 << 1) | (1 << 3) | (0x0F << 4) | (0x40 << 12));
    regs.write32(TIPG, 0x0060_200A);

    let link = regs.read32(STATUS) & 2 != 0;
    *NIC.lock() = Some(Nic { regs, rx_ring, rx_bufs, rx_next: 0, tx_ring, tx_bufs, tx_next: 0 });

    let desc: &'static NetDesc = Box::leak(Box::new(NetDesc { ctx: core::ptr::null_mut(), mac, _pad: [0; 2], mtu: 1500, send }));
    let iface = arcdrv::net_register(dev.id, desc);
    if iface < 0 {
        return iface;
    }
    IFACE.store(iface, Ordering::Relaxed);

    if dev.irq != IRQ_NONE && arcdrv::irq_attach(dev.id, dev.irq, irq, core::ptr::null_mut()) == OK {
        if let Some(nic) = NIC.lock().as_ref() {
            nic.regs.write32(IMS, (1 << 7) | (1 << 4) | (1 << 2)); // receive timer, overrun, link change
        }
    }
    // Interrupt routing can be wrong on some boards; poll as a safety net.
    arcdrv::spawn("e1000-rx", poller, core::ptr::null_mut());
    arcdrv::info!("link {}", if link { "up" } else { "down" });
    OK
}
