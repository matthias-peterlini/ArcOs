fn main() {
    let dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    println!("cargo:rerun-if-changed={dir}/kernel.ld");
    // A fixed-address kernel: linked where the boot loader puts it.
    println!("cargo:rustc-link-arg-bins=-T{dir}/kernel.ld");
    println!("cargo:rustc-link-arg-bins=--no-pie");
    println!("cargo:rustc-link-arg-bins=-z");
    println!("cargo:rustc-link-arg-bins=max-page-size=4096");
}
