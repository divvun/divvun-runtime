fn main() {
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").expect("CARGO_CFG_TARGET_OS not defined");

    if cfg!(windows) {
        //
    } else if target_os == "macos" {
        println!("cargo:rustc-link-lib=icucore");
        // println!("cargo:rustc-link-arg=-Wl,-all_load");
        // println!("cargo:rustc-link-arg=-Wl,-export_dynamic");
        // println!("cargo:rustc-link-arg=-Wl,-exported_symbol,_DRT_Bundle_fromBundle");
        // println!("cargo:rustc-link-arg=-Wl,-exported_symbol,_DRT_Bundle_create");
        const EXPORT: &[&str] = &[
            "_DRT_Bundle_fromBundle",
            "_DRT_Bundle_drop",
            "_DRT_Bundle_fromPath",
            "_DRT_Bundle_create",
            "_DRT_PipelineHandle_drop",
            "_DRT_Vec_drop",
            "_DRT_PipelineHandle_forward",
            "_DRT_Bundle_runPipeline",
        ];

        for exp in EXPORT {
            println!("cargo:rustc-link-arg=-Wl,-exported_symbol,{exp}");
        }
    } else if target_os == "linux" {
        println!("cargo:rustc-link-lib=icuuc");
        println!("cargo:rustc-link-lib=icuio");
        println!("cargo:rustc-link-lib=icudata");
        println!("cargo:rustc-link-lib=icui18n");
    } else {
        todo!("BAD OS")
    }
}
