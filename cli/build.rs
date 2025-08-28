fn main() {
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").expect("CARGO_CFG_TARGET_OS not defined");

    if cfg!(windows) {
        //
    } else if target_os == "macos" {
        // if let Ok(tmp_path) = std::env::var("TMP_PATH") {
        //     println!("cargo:rustc-link-search=native={}/lib", tmp_path);
        // }
        println!("cargo:rustc-link-lib=icucore");
    } else if target_os == "linux" {
        println!("cargo:rustc-link-lib=static=icuuc");
        println!("cargo:rustc-link-lib=static=icuio");
        println!("cargo:rustc-link-lib=static=icudata");
        println!("cargo:rustc-link-lib=static=icui18n");
    } else {
        todo!("BAD OS")
    }

    if target_os == "macos" {
        //
    } else if target_os == "linux" {
        //
    } else if target_os == "windows" {
        // std::fs::copy(
        //     artifact_path.join("python311.dll"),
        //     out_dir.join("python311.dll"),
        // )
        // .unwrap();
    } else {
        panic!("BAD OS")
    }

    match target_os.as_str() {
        "macos" => {
            println!("cargo:rustc-link-arg=-rdynamic");
        }
        _ => {}
    }
}
