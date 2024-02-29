use std::path::PathBuf;

fn main() {
    let out_dir =
        PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR not defined")).join("../../..");
    // panic!("{:?}", out_dir);
    let artifact_path =
        PathBuf::from(std::env::var("ARTIFACT_PATH").expect("ARTIFACT_PATH not defined"));

    let _lol = std::fs::remove_dir_all(out_dir.join("lib"));

    let target_os = std::env::var("CARGO_CFG_TARGET_OS").expect("CARGO_CFG_TARGET_OS not defined");

    if cfg!(windows) {
        let _ = std::fs::remove_dir_all(out_dir.join("lib"));
        let _ = std::fs::remove_dir_all(out_dir.join("DLLs"));
        fs_extra::copy_items(
            &[artifact_path.join("Lib"), artifact_path.join("DLLs")],
            &out_dir,
            &Default::default(),
        )
        .unwrap();
    } else if target_os == "macos" {
        std::fs::create_dir_all(out_dir.join("lib")).unwrap();
        fs_extra::dir::copy(
            artifact_path.join("lib").join("python3.11"),
            &out_dir.join("lib"),
            &Default::default(),
        )
        .unwrap();
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
        std::fs::copy(
            artifact_path.join("libpython3.a"),
            out_dir.join("libpython3.a"),
        )
        .unwrap();
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
            println!("cargo:rustc-link-arg=-rdynamic");
        }
        _ => {}
    }
}
