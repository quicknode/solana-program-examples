use std::path::Path;

fn main() {
    let so_path = Path::new("deploy/transfer-sol-cpi.so");

    if so_path.exists() {
        println!("cargo:rustc-cfg=has_asm_binary");
    } else {
        println!("cargo:warning=ASM binary not found at deploy/transfer-sol-cpi.so");
        println!("cargo:warning=Run `sbpf build` in the asm directory to generate it");
    }
}
