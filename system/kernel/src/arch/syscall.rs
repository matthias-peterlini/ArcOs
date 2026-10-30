//! The `syscall` instruction (used by Linux programs) and the user segment
//! bases (FS and GS) of each thread.
//!
//! `syscall` arrives on the user stack with interrupts masked. The entry
//! switches to the thread's kernel stack, builds the same [`TrapFrame`] an
//! interrupt would (vector [`VEC_LINUX`]) and joins the common interrupt
//! path, so it returns with `iretq` like every other trap. The kernel stack
//! and the saved user stack pointer live in the processor's own data
//! ([`super::smp::PerCpu`]), found through the kernel GS base.
//!
//! [`TrapFrame`]: super::idt::TrapFrame

use super::{cpu, gdt};
use core::arch::global_asm;

/// The vector [`super::idt::TrapFrame::vector`] holds for `syscall`.
pub const VEC_LINUX: u64 = 0x81;

const MSR_EFER: u32 = 0xC000_0080;
const MSR_STAR: u32 = 0xC000_0081;
const MSR_LSTAR: u32 = 0xC000_0082;
const MSR_FMASK: u32 = 0xC000_0084;
pub const MSR_FS_BASE: u32 = 0xC000_0100;
/// While the CPU is in the kernel, this holds the user GS base (the entry
/// paths run `swapgs`).
pub const MSR_KERNEL_GS_BASE: u32 = 0xC000_0102;

// gs:[0] is the kernel stack of the running thread, gs:[8] room for the
// user stack pointer. The `swapgs` back before joining the common path
// keeps the GS bases as an interrupt from user mode finds them.
global_asm!(
    r#"
    .global syscall_entry
syscall_entry:
    swapgs
    mov gs:[8], rsp
    mov rsp, gs:[0]
    push {user_data}
    push qword ptr gs:[8]
    swapgs
    push r11
    push {user_code}
    push rcx
    push 0
    push {vector}
    jmp isr_common
"#,
    user_data = const gdt::USER_DATA as u64,
    user_code = const gdt::USER_CODE as u64,
    vector = const VEC_LINUX,
);

extern "C" {
    fn syscall_entry();
}

pub fn init() {
    // SAFETY: enables `syscall` with the entry above; the selectors match
    // the GDT layout (kernel code, kernel data, user data, user code).
    unsafe {
        cpu::wrmsr(MSR_EFER, cpu::rdmsr(MSR_EFER) | 1);
        cpu::wrmsr(MSR_STAR, (0x10u64 << 48) | ((gdt::KERNEL_CODE as u64) << 32));
        cpu::wrmsr(MSR_LSTAR, syscall_entry as *const () as u64);
        // Cleared on entry: TF, IF, DF, NT, AC.
        cpu::wrmsr(MSR_FMASK, 0x0004_4700);
    }
}

/// The user FS and GS bases of the running thread.
pub fn user_bases() -> (u64, u64) {
    (cpu::rdmsr(MSR_FS_BASE), cpu::rdmsr(MSR_KERNEL_GS_BASE))
}

pub fn set_user_bases(fs: u64, gs: u64) {
    // SAFETY: user segment bases; the kernel uses neither FS nor GS.
    unsafe {
        cpu::wrmsr(MSR_FS_BASE, fs);
        cpu::wrmsr(MSR_KERNEL_GS_BASE, gs);
    }
}
