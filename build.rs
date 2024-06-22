use std::{collections::HashMap, path::PathBuf};

fn main() {
    let out_dir =
        PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR not defined")).join("../../..");
    let Ok(artifact_path) = std::env::var("ARTIFACT_PATH").map(PathBuf::from) else {
        return;
    };

    // let _lol = std::fs::remove_dir_all(out_dir.join("lib"));

    let target_os = std::env::var("CARGO_CFG_TARGET_OS").expect("CARGO_CFG_TARGET_OS not defined");

    if cfg!(windows) {
        // let _ = std::fs::remove_dir_all(out_dir.join("lib"));
        // let _ = std::fs::remove_dir_all(out_dir.join("DLLs"));
        // fs_extra::copy_items(
        //     &[artifact_path.join("Lib"), artifact_path.join("DLLs")],
        //     &out_dir,
        //     &Default::default(),
        // )
        // .unwrap();
    } else if target_os == "macos" {
        // std::fs::create_dir_all(out_dir.join("lib")).unwrap();
        // fs_extra::dir::copy(
        //     artifact_path.join("lib").join("python3.11"),
        //     &out_dir.join("lib"),
        //     &Default::default(),
        // )
        // .unwrap();
        let tmp_path = std::env::var("TMP_PATH").unwrap();
        println!("cargo:rustc-link-search=native={}/lib", tmp_path);
        println!("cargo:rustc-link-lib=static=icuuc");
        println!("cargo:rustc-link-lib=static=icuio");
        println!("cargo:rustc-link-lib=static=icudata");
        println!("cargo:rustc-link-lib=static=icui18n");
    } else if target_os == "linux" {
        std::fs::create_dir_all(out_dir.join("lib")).unwrap();

        fs_extra::dir::copy(
            artifact_path.join("lib").join("python3.11"),
            &out_dir.join("lib"),
            &Default::default(),
        )
        .unwrap();

        println!("cargo:rustc-link-lib=static=icuuc");
        println!("cargo:rustc-link-lib=static=icuio");
        println!("cargo:rustc-link-lib=static=icudata");
        println!("cargo:rustc-link-lib=static=icui18n");
    } else {
        todo!("BAD OS")
    }

    if target_os == "macos" {
        std::fs::copy(
            artifact_path.join("Python"),
            out_dir.join("libpython3.11.dylib"),
        )
        .unwrap();
    } else if target_os == "linux" {
        // std::fs::copy(
        //     artifact_path.join("lib").join("libpython3.a"),
        //     out_dir.join("libpython3.a"),
        // )
        // .unwrap();
    } else if target_os == "windows" {
        std::fs::copy(
            artifact_path.join("python311.dll"),
            out_dir.join("python311.dll"),
        )
        .unwrap();
    } else {
        panic!("BAD OS")
    }

    // Export symbols from built binaries. This is needed to ensure libpython's
    // symbols are exported. Without those symbols being exported, loaded extension
    // modules won't find the libpython symbols and won't be able to run.
    match target_os.as_str() {
        "linux" => {
            println!("cargo:rustc-link-arg=-Wl,-export-dynamic");
        }
        "macos" => {
            // println!("cargo:rustc-link-arg=-rdynamic");
        }
        _ => {}
    }
}
