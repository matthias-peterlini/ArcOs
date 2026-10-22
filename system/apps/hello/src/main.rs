//! The smallest arcOS program.

#![no_std]
#![no_main]

use arcrt::console::color::*;
use arcrt::println;

arcrt::main!(main);

fn main(args: &str) -> i32 {
    // Demonstrations of isolation: the system survives both.
    match args.trim() {
        "--crash" => {
            println!("writing to address 0...");
            // SAFETY: deliberately invalid; the kernel stops this program.
            unsafe { core::ptr::write_volatile(core::ptr::null_mut::<u64>(), 1) };
        }
        "--spin" => {
            println!("spinning forever; press Ctrl+C");
            loop {
                core::hint::spin_loop();
            }
        }
        _ => {}
    }
    let t = arcrt::time::date(arcrt::time::now());
    println!("{ACCENT}Hello from user mode!{RESET}");
    println!("I am a program running in ring 3, in my own address space.");
    println!("Today is {:04}-{:02}-{:02}, my folder is {}.", t.year, t.month, t.day, arcrt::fs::current_dir());
    let words = arcrt::split_args(args);
    if !words.is_empty() {
        println!("You gave me {} argument(s):", words.len());
        for (i, w) in words.iter().enumerate() {
            println!("  {DIM}{i}{RESET} {w}");
        }
    }
    0
}
