use std::path::PathBuf;

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
        // std::fs::remove_dir_all(out_dir.join("lib")).unwrap_or(());
        // std::fs::create_dir_all(out_dir.join("lib")).unwrap();
        // fs_extra::dir::copy(
        //     artifact_path.join("lib").join("python3.11"),
        //     &out_dir.join("lib"),
        //     &CopyOptions {
        //         overwrite: true,
        //         ..Default::default()
        //     },
        // )
        // .unwrap();
        let tmp_path = std::env::var("TMP_PATH").unwrap();
        println!("cargo:rustc-link-search=native={}/lib", tmp_path);
        // println!("cargo:rustc-link-search=native=/opt/homebrew/lib");
        println!("cargo:rustc-link-lib=static=icuuc");
        println!("cargo:rustc-link-lib=static=icuio");
        println!("cargo:rustc-link-lib=static=icudata");
        println!("cargo:rustc-link-lib=static=icui18n");
    } else if target_os == "linux" {
        println!("cargo:rustc-link-lib=static=icuuc");
        println!("cargo:rustc-link-lib=static=icuio");
        println!("cargo:rustc-link-lib=static=icudata");
        println!("cargo:rustc-link-lib=static=icui18n");
    } else {
        todo!("BAD OS")
    }
    // println!("cargo:rustc-link-search=native={}/lib", tmp_path);
    // println!("cargo:rustc-link-lib=static=omp");
    // println!("cargo:rustc-link-search=native=/opt/homebrew/opt/protobuf@21/lib");
    // println!("cargo:rustc-link-search=native=/opt/libtorch/lib");
    // println!("cargo:rustc-link-lib=static=protobuf-lite");
    // println!("cargo:rustc-link-lib=static=protobuf");
    // println!("cargo:rustc-link-lib=static=protoc");
    // println!("cargo:rustc-link-lib=static=onnx");
    // println!("cargo:rustc-link-lib=static=onnx_proto");
    // println!("cargo:rustc-link-lib=protobuf-lite");
    // println!("cargo:rustc-link-lib=protobuf");
    // println!("cargo:rustc-link-lib=protoc");
    // println!("cargo:rustc-link-lib=onnx");
    // println!("cargo:rustc-link-lib=onnx_proto");

    if target_os == "macos" {
        // std::fs::copy(
        //     artifact_path.join("Python"),
        //     out_dir.join("libpython3.11.dylib"),
        // )
        // .unwrap();
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

    match target_os.as_str() {
        "macos" => {
            // println!("cargo:rustc-link-arg=-Wl,-all_load");
            println!("cargo:rustc-link-arg=-rdynamic");
        }
        _ => {}
    }
}
