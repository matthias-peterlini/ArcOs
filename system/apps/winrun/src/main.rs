//! Runs a Windows x86-64 program (`.exe`) on arcOS.
//!
//! `winrun` is an ordinary arcOS program that loads a Windows executable into
//! its own address space, fills in the functions the executable imports from
//! `kernel32.dll` and `msvcrt.dll` with its own code, and jumps to the
//! program's entry point. The Windows program then runs in the same process:
//! when it calls, say, `WriteFile` or `printf`, it calls into [`win`], which
//! turns the request into arcOS system calls.
//!
//! Only console programs are handled, and only a useful part of the Windows
//! API — enough for programs built with MinGW-w64 (`x86_64-w64-mingw32-gcc`).
//! There is no graphics, no registry, no real processes or threads. It is the
//! same idea as Wine, in miniature.

#![no_std]
#![no_main]

extern crate alloc;

mod pe;
mod win;

use alloc::string::String;
use arcrt::{eprintln, println};

arcrt::main!(main);

fn main(args: &str) -> i32 {
    let args = args.trim();
    if args.is_empty() || args == "--help" {
        println!("usage: winrun <program.exe> [arguments]");
        println!("runs a Windows x86-64 console program.");
        return if args.is_empty() { 2 } else { 0 };
    }
    // The first word is the program; the rest are its arguments.
    let parts = arcrt::split_args(args);
    let path = &parts[0];
    let image = match arcrt::fs::read(path) {
        Ok(data) => data,
        Err(e) => {
            eprintln!("winrun: {path}: {}", e.as_str());
            return 1;
        }
    };
    // argv[0] is the program name as given; the rest follow.
    let argv: alloc::vec::Vec<String> = parts.clone();
    match pe::run(&image, &argv) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("winrun: {path}: {e}");
            1
        }
    }
}
