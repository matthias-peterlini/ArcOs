//! The binary interface between the kernel and driver modules.
//!
//! Rules that keep the ABI stable:
//! * every type is `#[repr(C)]` and only uses fixed-size integers, raw
//!   pointers and `extern "C"` function pointers;
//! * [`KernelApi`] only ever grows at the end; its `size` field tells a module
//!   which entries exist, so new kernels run old modules;
//! * an incompatible change bumps [`ABI_VERSION`].

use core::ffi::c_void;

/// Current version of the driver ABI.
pub const ABI_VERSION: u32 = 1;

/// `DriverInfo::magic`: "ARDV".
pub const DRIVER_MAGIC: u32 = 0x5644_5241;

/// Name of the symbol every driver module exports as its ELF entry point.
pub const ENTRY_SYMBOL: &str = "arc_driver_entry";

/// Signature of a module's entry point.
pub type DriverEntry = unsafe extern "C" fn(api: *const KernelApi) -> *const DriverInfo;

// ----------------------------------------------------------------------------
// Status codes

/// Result of a kernel or driver call: 0 or positive is success.
pub type Status = i32;

pub const OK: Status = 0;
/// `probe`: the device is not handled by this driver.
pub const E_NODEV: Status = -1;
pub const E_IO: Status = -2;
pub const E_NOMEM: Status = -3;
pub const E_INVAL: Status = -4;
pub const E_UNSUPPORTED: Status = -5;
pub const E_TIMEOUT: Status = -6;
pub const E_BUSY: Status = -7;

/// Short human-readable name of a status code.
pub fn status_name(status: Status) -> &'static str {
    match status {
        OK => "ok",
        E_NODEV => "no such device",
        E_IO => "I/O error",
        E_NOMEM => "out of memory",
        E_INVAL => "invalid argument",
        E_UNSUPPORTED => "not supported",
        E_TIMEOUT => "timed out",
        E_BUSY => "busy",
        _ => "error",
    }
}

// ----------------------------------------------------------------------------
// Strings

/// A borrowed UTF-8 string that crosses the ABI.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Str {
    pub ptr: *const u8,
    pub len: usize,
}

impl Str {
    pub const fn new(s: &'static str) -> Str {
        Str { ptr: s.as_ptr(), len: s.len() }
    }

    pub const fn empty() -> Str {
        Str { ptr: core::ptr::null(), len: 0 }
    }

    /// # Safety
    /// `ptr..ptr+len` must be valid UTF-8 that outlives the returned slice.
    pub unsafe fn as_str<'a>(&self) -> &'a str {
        if self.ptr.is_null() || self.len == 0 {
            return "";
        }
        // SAFETY: guaranteed by the caller.
        unsafe { core::str::from_utf8_unchecked(core::slice::from_raw_parts(self.ptr, self.len)) }
    }
}

// SAFETY: a Str only ever points at immutable, 'static or kernel-owned data.
unsafe impl Send for Str {}
unsafe impl Sync for Str {}

// ----------------------------------------------------------------------------
// Devices

pub const BUS_PCI: u32 = 1;
/// Fixed, non-enumerable hardware of the platform (PS/2, legacy ATA, ...).
pub const BUS_PLATFORM: u32 = 2;

pub const RES_NONE: u32 = 0;
pub const RES_MEMORY: u32 = 1;
pub const RES_IO: u32 = 2;

/// A memory or I/O port range of a device (a PCI BAR, or a fixed range).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct Resource {
    pub kind: u32,
    pub flags: u32,
    pub base: u64,
    pub size: u64,
}

/// `Resource::flags`: prefetchable memory.
pub const RES_PREFETCH: u32 = 1;
/// `Resource::flags`: 64-bit memory BAR.
pub const RES_64BIT: u32 = 2;

/// No interrupt line.
pub const IRQ_NONE: u8 = 0xFF;

/// Everything the kernel knows about a device, passed to `probe`.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct DeviceInfo {
    /// Kernel handle of the device, used in every device-related call.
    pub id: u32,
    pub bus: u32,
    pub vendor: u16,
    pub device: u16,
    pub subsystem_vendor: u16,
    pub subsystem: u16,
    pub class: u8,
    pub subclass: u8,
    pub prog_if: u8,
    pub revision: u8,
    /// Legacy interrupt line (PIC IRQ), or [`IRQ_NONE`].
    pub irq: u8,
    pub _pad: [u8; 3],
    /// PCI location: bus << 8 | slot << 3 | function.
    pub location: u32,
    pub resources: [Resource; 6],
    /// Platform device id ("ps2-keyboard"), NUL-padded. Empty on PCI.
    pub name: [u8; 32],
}

impl DeviceInfo {
    pub const fn empty() -> DeviceInfo {
        DeviceInfo {
            id: 0,
            bus: 0,
            vendor: 0,
            device: 0,
            subsystem_vendor: 0,
            subsystem: 0,
            class: 0,
            subclass: 0,
            prog_if: 0,
            revision: 0,
            irq: IRQ_NONE,
            _pad: [0; 3],
            location: 0,
            resources: [Resource { kind: 0, flags: 0, base: 0, size: 0 }; 6],
            name: [0; 32],
        }
    }

    /// The platform id as a string.
    pub fn name(&self) -> &str {
        let len = self.name.iter().position(|&b| b == 0).unwrap_or(self.name.len());
        core::str::from_utf8(&self.name[..len]).unwrap_or("")
    }

    pub fn set_name(&mut self, name: &str) {
        self.name = [0; 32];
        let n = name.len().min(31);
        self.name[..n].copy_from_slice(&name.as_bytes()[..n]);
    }

    /// First I/O port resource at or after `index`.
    pub fn io(&self, index: usize) -> Option<u16> {
        self.resources.iter().filter(|r| r.kind == RES_IO).nth(index).map(|r| r.base as u16)
    }
}

// ----------------------------------------------------------------------------
// Block devices

/// `BlockDesc::flags`: the medium is read-only.
pub const BLOCK_READONLY: u32 = 1;
/// `BlockDesc::flags`: the medium can be removed (USB stick, CD).
pub const BLOCK_REMOVABLE: u32 = 2;

pub type BlockIo = unsafe extern "C" fn(ctx: *mut c_void, lba: u64, count: u32, buf: *mut u8) -> Status;
pub type BlockFlush = unsafe extern "C" fn(ctx: *mut c_void) -> Status;

/// A disk a driver hands to the kernel. The kernel serialises calls per disk.
#[repr(C)]
pub struct BlockDesc {
    pub ctx: *mut c_void,
    pub sector_size: u32,
    pub flags: u32,
    pub sector_count: u64,
    /// Reads `count` sectors starting at `lba` into `buf`.
    pub read: BlockIo,
    /// Writes `count` sectors starting at `lba` from `buf`.
    pub write: BlockIo,
    pub flush: Option<BlockFlush>,
    /// Model name, NUL-padded.
    pub model: [u8; 40],
}

// ----------------------------------------------------------------------------
// Input

/// Key press/release: `code` is a USB HID usage (page 7), `value` one of
/// [`KEY_RELEASED`], [`KEY_PRESSED`], [`KEY_REPEAT`].
pub const INPUT_KEY: u32 = 1;
/// Relative motion: `code` is [`AXIS_X`], [`AXIS_Y`] or [`AXIS_WHEEL`].
pub const INPUT_REL: u32 = 2;
/// Pointer button: `code` 0 = left, 1 = right, 2 = middle; `value` 1/0.
pub const INPUT_BUTTON: u32 = 3;

pub const KEY_RELEASED: i32 = 0;
pub const KEY_PRESSED: i32 = 1;
pub const KEY_REPEAT: i32 = 2;

pub const AXIS_X: u32 = 0;
pub const AXIS_Y: u32 = 1;
pub const AXIS_WHEEL: u32 = 2;

// ----------------------------------------------------------------------------
// Network

pub type NetSend = unsafe extern "C" fn(ctx: *mut c_void, frame: *const u8, len: usize) -> Status;

/// A network interface a driver hands to the kernel. Received frames are
/// passed to `KernelApi::net_receive`.
#[repr(C)]
pub struct NetDesc {
    pub ctx: *mut c_void,
    pub mac: [u8; 6],
    pub _pad: [u8; 2],
    pub mtu: u32,
    pub send: NetSend,
}

// ----------------------------------------------------------------------------
// Logging

pub const LOG_ERROR: u32 = 1;
pub const LOG_WARN: u32 = 2;
pub const LOG_INFO: u32 = 3;
pub const LOG_DEBUG: u32 = 4;

pub type IrqHandler = unsafe extern "C" fn(ctx: *mut c_void);
pub type ThreadEntry = unsafe extern "C" fn(ctx: *mut c_void);

/// The services the kernel offers a driver module.
#[repr(C)]
pub struct KernelApi {
    pub abi_version: u32,
    /// Size of this structure in bytes: entries past it do not exist.
    pub size: u32,

    pub log: unsafe extern "C" fn(level: u32, msg: *const u8, len: usize),

    // Memory
    pub alloc: unsafe extern "C" fn(size: usize, align: usize) -> *mut u8,
    pub free: unsafe extern "C" fn(ptr: *mut u8, size: usize, align: usize),
    /// Maps device memory (uncached) and returns its virtual address.
    pub map_mmio: unsafe extern "C" fn(phys: u64, size: usize) -> *mut u8,
    /// Physically contiguous, zeroed, page-aligned memory below 4 GiB for DMA.
    pub dma_alloc: unsafe extern "C" fn(size: usize, phys: *mut u64) -> *mut u8,
    pub dma_free: unsafe extern "C" fn(ptr: *mut u8, size: usize),
    pub virt_to_phys: unsafe extern "C" fn(ptr: *const u8) -> u64,

    // PCI configuration space of a device (width 1, 2 or 4).
    pub pci_read: unsafe extern "C" fn(device: u32, offset: u16, width: u8) -> u32,
    pub pci_write: unsafe extern "C" fn(device: u32, offset: u16, width: u8, value: u32),

    // Interrupts
    pub irq_attach: unsafe extern "C" fn(device: u32, irq: u8, handler: IrqHandler, ctx: *mut c_void) -> Status,
    pub irq_detach: unsafe extern "C" fn(device: u32, irq: u8),

    // Time and threads
    pub time_ms: unsafe extern "C" fn() -> u64,
    pub sleep_ms: unsafe extern "C" fn(ms: u64),
    pub yield_now: unsafe extern "C" fn(),
    pub spawn: unsafe extern "C" fn(name: *const u8, name_len: usize, entry: ThreadEntry, ctx: *mut c_void) -> Status,

    // Device classes
    /// Registers a disk; returns its number (`disk<N>`) or an error.
    pub block_register: unsafe extern "C" fn(device: u32, desc: *const BlockDesc) -> Status,
    /// Reports an input event (see `INPUT_*`).
    pub input_event: unsafe extern "C" fn(device: u32, kind: u32, code: u32, value: i32),
    /// Registers a network interface; returns its number (`net<N>`).
    pub net_register: unsafe extern "C" fn(device: u32, desc: *const NetDesc) -> Status,
    /// Hands a received Ethernet frame to the kernel.
    pub net_receive: unsafe extern "C" fn(iface: u32, frame: *const u8, len: usize),
}

/// What a module tells the kernel about itself.
#[repr(C)]
pub struct DriverInfo {
    pub magic: u32,
    pub abi_version: u32,
    pub name: Str,
    pub version: Str,
    /// Called for every device whose manifest rule matched. Returns [`OK`]
    /// once the driver owns the device, [`E_NODEV`] to decline it.
    pub probe: unsafe extern "C" fn(device: *const DeviceInfo) -> Status,
    /// Releases a device before the module is unloaded. Without it the
    /// module can never be unloaded.
    pub remove: Option<unsafe extern "C" fn(device: u32)>,
}

// SAFETY: DriverInfo is immutable static data.
unsafe impl Sync for DriverInfo {}
