use std::path::PathBuf;

fn main() {
    let out_dir =
        PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR not defined")).join("../../..");
    // panic!("{:?}", out_dir);
    let artifact_path =
        PathBuf::from(std::env::var("ARTIFACT_PATH").expect("ARTIFACT_PATH not defined"));

    std::fs::remove_dir_all(out_dir.join("lib")).unwrap();
    std::fs::create_dir_all(out_dir.join("lib")).unwrap();
    std::fs::rename(
        artifact_path.join("stdlib"),
        out_dir.join("lib").join("python3.11"),
    )
    .unwrap();
    std::fs::rename(artifact_path.join("libpython3.11.dylib"), out_dir.join("libpython3.11.dylib")).unwrap();

    let target_os = std::env::var("CARGO_CFG_TARGET_OS").expect("CARGO_CFG_TARGET_OS not defined");

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
