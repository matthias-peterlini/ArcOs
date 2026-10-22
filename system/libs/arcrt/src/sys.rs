//! Raw system calls (see `arcdk::syscall`).

pub use arcdk::syscall::*;
use core::arch::asm;

#[inline]
pub fn call(n: u64, a: u64, b: u64, c: u64, d: u64, e: u64) -> i64 {
    let ret: i64;
    // SAFETY: int 0x80 enters the kernel, which validates every argument.
    unsafe {
        asm!("int 0x80", inlateout("rax") n as i64 => ret, in("rdi") a, in("rsi") b, in("rdx") c, in("r10") d, in("r8") e, options(nostack));
    }
    ret
}

/// Turns a raw result into `Ok(value)` or the error.
pub fn check(ret: i64) -> crate::Result<u64> {
    if ret < 0 {
        Err(Error::from_code(ret))
    } else {
        Ok(ret as u64)
    }
}
