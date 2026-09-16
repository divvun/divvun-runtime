fn main() {
    let target = std::env::var("TARGET").unwrap();
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").expect("CARGO_CFG_TARGET_OS not defined");

    // ICU linking is handled by cg3-rs and hfst-rs dependencies

    // The exported-symbol demands are only satisfiable when the ffi feature
    // compiles the C-ABI surface into the library.
    let ffi = std::env::var_os("CARGO_FEATURE_FFI").is_some();
    if ffi && target_os == "macos" {
        const EXPORT: &[&str] = &[
            "_DRT_Bundle_fromBundle",
            "_DRT_Bundle_drop",
            "_DRT_Bundle_fromPath",
            "_DRT_Bundle_create",
            "_DRT_PipelineHandle_drop",
            "_DRT_Vec_drop",
            "_DRT_PipelineHandle_forward",
            "_DRT_Bundle_runPipeline",
            "_DRT_Bundle_metadataAttr",
            "_DRT_Bundle_metadataKeys",
        ];

        for exp in EXPORT {
            println!("cargo:rustc-link-arg=-Wl,-exported_symbol,{exp}");
        }
    }
}
