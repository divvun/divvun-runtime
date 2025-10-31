fn main() {
    let target = std::env::var("TARGET").unwrap();
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").expect("CARGO_CFG_TARGET_OS not defined");

    println!("cargo:rustc-link-search=native=../.x/sysroot/{target}/lib");

    if cfg!(windows) {
        //
    } else if target_os == "macos" {
    } else if cfg!(unix) {
        println!("cargo:rustc-link-lib=static=icuuc");
        println!("cargo:rustc-link-lib=static=icuio");
        println!("cargo:rustc-link-lib=static=icudata");
        println!("cargo:rustc-link-lib=static=icui18n");

        if target_os == "macos" {
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
        }
    } else {
        todo!("BAD OS")
    }
}
