use std::path::Path;

fn main() {
    let so_path = Path::new("deploy/program.so");

    if so_path.exists() {
        println!("cargo:rustc-cfg=has_asm_binary");
    } else {
        println!("cargo:warning=ASM binary not found at deploy/program.so");
        println!("cargo:warning=Run `sbpf build` in the asm directory to generate it");
    }
}
