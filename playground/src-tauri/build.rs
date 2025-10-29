fn main() {
    println!("cargo:rustc-link-search=native=../../../.x/sysroot/{target}/lib");
    tauri_build::build()
}
