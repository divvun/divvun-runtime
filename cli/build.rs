fn main() {
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").expect("CARGO_CFG_TARGET_OS not defined");

    if cfg!(windows) {
        //
    } else if target_os == "macos" {
        println!("cargo:rustc-link-lib=icucore");
    } else if target_os == "linux" {
        println!("cargo:rustc-link-lib=icuuc");
        println!("cargo:rustc-link-lib=icuio");
        println!("cargo:rustc-link-lib=icudata");
        println!("cargo:rustc-link-lib=icui18n");
    } else {
        todo!("BAD OS")
    }
}
