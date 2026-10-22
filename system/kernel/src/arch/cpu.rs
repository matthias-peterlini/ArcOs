//! Thin wrappers over privileged x86-64 instructions.

use core::arch::asm;

#[inline]
pub fn inb(port: u16) -> u8 {
    let v: u8;
    // SAFETY: port I/O in ring 0 has no memory effects.
    unsafe { asm!("in al, dx", out("al") v, in("dx") port, options(nomem, nostack, preserves_flags)) };
    v
}

#[inline]
pub fn outb(port: u16, v: u8) {
    // SAFETY: see inb.
    unsafe { asm!("out dx, al", in("dx") port, in("al") v, options(nomem, nostack, preserves_flags)) };
}

#[inline]
pub fn inw(port: u16) -> u16 {
    let v: u16;
    // SAFETY: see inb.
    unsafe { asm!("in ax, dx", out("ax") v, in("dx") port, options(nomem, nostack, preserves_flags)) };
    v
}

#[inline]
pub fn outw(port: u16, v: u16) {
    // SAFETY: see inb.
    unsafe { asm!("out dx, ax", in("dx") port, in("ax") v, options(nomem, nostack, preserves_flags)) };
}

#[inline]
pub fn inl(port: u16) -> u32 {
    let v: u32;
    // SAFETY: see inb.
    unsafe { asm!("in eax, dx", out("eax") v, in("dx") port, options(nomem, nostack, preserves_flags)) };
    v
}

#[inline]
pub fn outl(port: u16, v: u32) {
    // SAFETY: see inb.
    unsafe { asm!("out dx, eax", in("dx") port, in("eax") v, options(nomem, nostack, preserves_flags)) };
}

#[inline]
pub fn io_wait() {
    outb(0x80, 0);
}

/// Disables interrupts and returns whether they were enabled.
#[inline]
pub fn irq_save() -> bool {
    let flags: u64;
    // SAFETY: reads RFLAGS and masks interrupts.
    unsafe { asm!("pushfq; pop {}; cli", out(reg) flags, options(nomem)) };
    flags & (1 << 9) != 0
}

#[inline]
pub fn irq_restore(enabled: bool) {
    if enabled {
        // SAFETY: re-enables interrupts that were enabled before irq_save.
        unsafe { asm!("sti", options(nomem, nostack)) };
    }
}

#[inline]
pub fn irq_enabled() -> bool {
    let flags: u64;
    // SAFETY: reads RFLAGS.
    unsafe { asm!("pushfq; pop {}", out(reg) flags, options(nomem, preserves_flags)) };
    flags & (1 << 9) != 0
}

#[inline]
pub fn enable_irq() {
    // SAFETY: ring 0.
    unsafe { asm!("sti", options(nomem, nostack)) };
}

#[inline]
pub fn disable_irq() {
    // SAFETY: ring 0.
    unsafe { asm!("cli", options(nomem, nostack)) };
}

/// Waits for the next interrupt (interrupts must be enabled to wake up).
#[inline]
pub fn halt() {
    // SAFETY: ring 0.
    unsafe { asm!("hlt", options(nomem, nostack)) };
}

/// Atomically enables interrupts and halts: no interrupt can slip in between.
#[inline]
pub fn enable_and_halt() {
    // SAFETY: ring 0.
    unsafe { asm!("sti; hlt", options(nomem, nostack)) };
}

pub fn halt_forever() -> ! {
    loop {
        disable_irq();
        halt();
    }
}

#[inline]
pub fn read_cr2() -> u64 {
    let v: u64;
    // SAFETY: ring 0.
    unsafe { asm!("mov {}, cr2", out(reg) v, options(nomem, nostack, preserves_flags)) };
    v
}

#[inline]
pub fn read_cr3() -> u64 {
    let v: u64;
    // SAFETY: ring 0.
    unsafe { asm!("mov {}, cr3", out(reg) v, options(nomem, nostack, preserves_flags)) };
    v
}

/// # Safety
/// `pml4` must be a valid top-level page table that maps the running code.
#[inline]
pub unsafe fn write_cr3(pml4: u64) {
    // SAFETY: guaranteed by the caller.
    unsafe { asm!("mov cr3, {}", in(reg) pml4, options(nostack, preserves_flags)) };
}

#[inline]
pub fn invlpg(addr: u64) {
    // SAFETY: only drops a TLB entry.
    unsafe { asm!("invlpg [{}]", in(reg) addr, options(nostack, preserves_flags)) };
}

#[inline]
pub fn rdmsr(msr: u32) -> u64 {
    let (lo, hi): (u32, u32);
    // SAFETY: ring 0; callers only use architectural MSRs.
    unsafe { asm!("rdmsr", in("ecx") msr, out("eax") lo, out("edx") hi, options(nomem, nostack, preserves_flags)) };
    (hi as u64) << 32 | lo as u64
}

/// # Safety
/// Writing an MSR can change how the CPU behaves in any way.
#[inline]
pub unsafe fn wrmsr(msr: u32, v: u64) {
    // SAFETY: guaranteed by the caller.
    unsafe { asm!("wrmsr", in("ecx") msr, in("eax") v as u32, in("edx") (v >> 32) as u32, options(nostack, preserves_flags)) };
}

#[derive(Clone, Copy, Debug)]
pub struct CpuidResult {
    pub eax: u32,
    pub ebx: u32,
    pub ecx: u32,
    pub edx: u32,
}

pub fn cpuid(leaf: u32, sub: u32) -> CpuidResult {
    let r = core::arch::x86_64::__cpuid_count(leaf, sub);
    CpuidResult { eax: r.eax, ebx: r.ebx, ecx: r.ecx, edx: r.edx }
}

/// The CPU brand string ("QEMU Virtual CPU", "Intel(R) Core(TM) ...").
pub fn brand(out: &mut [u8; 48]) -> &str {
    if cpuid(0x8000_0000, 0).eax < 0x8000_0004 {
        return "unknown CPU";
    }
    for i in 0..3 {
        let r = cpuid(0x8000_0002 + i, 0);
        for (j, v) in [r.eax, r.ebx, r.ecx, r.edx].iter().enumerate() {
            out[i as usize * 16 + j * 4..i as usize * 16 + j * 4 + 4].copy_from_slice(&v.to_le_bytes());
        }
    }
    let len = out.iter().position(|&b| b == 0).unwrap_or(48);
    core::str::from_utf8(&out[..len]).unwrap_or("unknown CPU").trim()
}

#[inline]
pub fn rdtsc() -> u64 {
    // SAFETY: rdtsc has no side effects.
    unsafe { core::arch::x86_64::_rdtsc() }
}
