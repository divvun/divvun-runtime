fn main() {
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").expect("CARGO_CFG_TARGET_OS not defined");

    if let Ok(path) = std::env::var("LIBTORCH") {
        println!("cargo:rustc-link-search=native={}/lib", path);
    } else {
        if target_os == "macos" {
            println!("cargo:rustc-link-search=native=/opt/libtorch/lib");
        } else {
            panic!("Unsupported target OS: {}", target_os);
        }
    }

    if cfg!(windows) {
        //
    } else if target_os == "macos" {
        println!("cargo:rustc-link-lib=icucore");
    } else if target_os == "linux" {
        println!("cargo:rustc-link-lib=static=icuuc");
        println!("cargo:rustc-link-lib=static=icuio");
        println!("cargo:rustc-link-lib=static=icudata");
        println!("cargo:rustc-link-lib=static=icui18n");
    } else {
        todo!("BAD OS")
    }
}
