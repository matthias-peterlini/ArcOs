//! Arc Development Kit: the contracts between the kernel and the code it runs.
//!
//! For drivers:
//!
//! * [`abi`] — the `#[repr(C)]` types a driver module and the kernel exchange.
//!   The kernel hands every module a [`abi::KernelApi`] table; the module
//!   answers with a [`abi::DriverInfo`]. Nothing else crosses the boundary, so
//!   a module built against ABI version N keeps working on any kernel that
//!   still accepts N.
//! * [`manifest`] — the text manifest of a driver: name, version and the
//!   hardware it drives. The kernel matches devices against manifests to
//!   install drivers automatically, without loading any code first.
//! * [`package`] — the `.adp` driver package: header, manifest and module image.
//!
//! For user programs:
//!
//! * [`syscall`] — system call numbers, error codes and key codes.
//!
//! The run-time support for driver modules (heap, panic handler, entry
//! point, hardware helpers) is the separate `arcdrv` crate.
//!
//! The crate is `no_std` and has no dependencies, so the kernel, the drivers
//! and the host tools all share the same code.

#![cfg_attr(not(test), no_std)]

pub mod abi;
pub mod crc32;
pub mod manifest;
pub mod package;
pub mod syscall;

pub use abi::*;
