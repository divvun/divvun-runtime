fn main() {
    let target = std::env::var("TARGET").unwrap();
    println!("cargo:rustc-link-search=native=../../../.x/sysroot/{target}/lib");
    tauri_build::build()
}
