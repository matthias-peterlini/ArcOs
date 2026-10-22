//! Locks.
//!
//! * [`IrqLock`] masks interrupts while held: for state shared with interrupt
//!   handlers and for short critical sections. Never block while holding it.
//! * [`Mutex`] leaves interrupts on and yields to other threads while it is
//!   contended: for long operations such as disk and file system access.
//!   Never take it from an interrupt handler.

use crate::arch::cpu;
use core::cell::UnsafeCell;
use core::ops::{Deref, DerefMut};
use core::sync::atomic::{AtomicBool, Ordering};

pub struct IrqLock<T: ?Sized> {
    locked: AtomicBool,
    value: UnsafeCell<T>,
}

// SAFETY: the lock serialises access to the value.
unsafe impl<T: ?Sized + Send> Sync for IrqLock<T> {}
unsafe impl<T: ?Sized + Send> Send for IrqLock<T> {}

pub struct IrqGuard<'a, T: ?Sized> {
    lock: &'a IrqLock<T>,
    irq: bool,
}

impl<T> IrqLock<T> {
    pub const fn new(value: T) -> IrqLock<T> {
        IrqLock { locked: AtomicBool::new(false), value: UnsafeCell::new(value) }
    }
}

impl<T: ?Sized> IrqLock<T> {
    pub fn lock(&self) -> IrqGuard<'_, T> {
        let irq = cpu::irq_save();
        while self.locked.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
            core::hint::spin_loop();
        }
        IrqGuard { lock: self, irq }
    }

    pub fn try_lock(&self) -> Option<IrqGuard<'_, T>> {
        let irq = cpu::irq_save();
        if self.locked.compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed).is_ok() {
            Some(IrqGuard { lock: self, irq })
        } else {
            cpu::irq_restore(irq);
            None
        }
    }

    /// Releases a lock whose holder can no longer release it (panic path).
    ///
    /// # Safety
    /// Only when the holder will never touch the value again.
    pub unsafe fn force_unlock(&self) {
        self.locked.store(false, Ordering::Release);
    }
}

impl<T: ?Sized> Deref for IrqGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        // SAFETY: the lock is held.
        unsafe { &*self.lock.value.get() }
    }
}

impl<T: ?Sized> DerefMut for IrqGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        // SAFETY: the lock is held.
        unsafe { &mut *self.lock.value.get() }
    }
}

impl<T: ?Sized> Drop for IrqGuard<'_, T> {
    fn drop(&mut self) {
        self.lock.locked.store(false, Ordering::Release);
        cpu::irq_restore(self.irq);
    }
}

pub struct Mutex<T: ?Sized> {
    locked: AtomicBool,
    value: UnsafeCell<T>,
}

// SAFETY: the lock serialises access to the value.
unsafe impl<T: ?Sized + Send> Sync for Mutex<T> {}
unsafe impl<T: ?Sized + Send> Send for Mutex<T> {}

pub struct MutexGuard<'a, T: ?Sized> {
    lock: &'a Mutex<T>,
}

impl<T> Mutex<T> {
    pub const fn new(value: T) -> Mutex<T> {
        Mutex { locked: AtomicBool::new(false), value: UnsafeCell::new(value) }
    }
}

impl<T: ?Sized> Mutex<T> {
    pub fn lock(&self) -> MutexGuard<'_, T> {
        while self.locked.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
            crate::task::yield_now();
        }
        MutexGuard { lock: self }
    }

    pub fn try_lock(&self) -> Option<MutexGuard<'_, T>> {
        self.locked
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
            .then_some(MutexGuard { lock: self })
    }
}

impl<T: ?Sized> Deref for MutexGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        // SAFETY: the lock is held.
        unsafe { &*self.lock.value.get() }
    }
}

impl<T: ?Sized> DerefMut for MutexGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        // SAFETY: the lock is held.
        unsafe { &mut *self.lock.value.get() }
    }
}

impl<T: ?Sized> Drop for MutexGuard<'_, T> {
    fn drop(&mut self) {
        self.lock.locked.store(false, Ordering::Release);
    }
}
