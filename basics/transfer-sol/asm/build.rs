// The tests embed the assembled program with include_bytes!, which needs the
// file to exist at compile time. `sbpf build` produces it, and it is gitignored,
// so a fresh checkout has no binary and the test module cannot compile. Set a
// cfg when the binary is present and let the tests key off it: present, they
// compile and run; absent, they are left out and the crate still builds.
use std::path::Path;

const BINARY: &str = "deploy/transfer-sol-cpi.so";

fn main() {
    // Declare the cfg so `-D warnings` builds do not fail on unexpected_cfgs.
    println!("cargo::rustc-check-cfg=cfg(has_asm_binary)");
    // Re-run when the binary appears or changes, so the cfg never goes stale.
    println!("cargo::rerun-if-changed={BINARY}");

    if Path::new(BINARY).exists() {
        println!("cargo::rustc-cfg=has_asm_binary");
    } else {
        println!("cargo::warning=ASM binary not found at {BINARY}: tests skipped");
        println!("cargo::warning=Run `sbpf build` in this directory to generate it");
    }
}
