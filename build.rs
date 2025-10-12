use vergen_git2::{BuildBuilder, CargoBuilder, Emitter, Git2Builder, RustcBuilder};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").expect("CARGO_CFG_TARGET_OS not defined");

    if cfg!(windows) {
        println!("cargo:rustc-link-lib=icudt");
        println!("cargo:rustc-link-lib=icuin");
        println!("cargo:rustc-link-lib=icudata");
        println!("cargo:rustc-link-lib=icui18n");
    } else if target_os == "macos" {
        println!("cargo:rustc-link-lib=icucore");
        // println!("cargo:rustc-link-arg=-Wl,-export_dynamic");
        println!("cargo:rustc-link-arg=-Wl,-all_load");
    } else if target_os == "linux" {
        println!("cargo:rustc-link-lib=icuuc");
        println!("cargo:rustc-link-lib=icuio");
        println!("cargo:rustc-link-lib=icudata");
        println!("cargo:rustc-link-lib=icui18n");
    } else {
        todo!("BAD OS")
    }

    let build = BuildBuilder::all_build()?;
    let cargo = CargoBuilder::all_cargo()?;
    let rustc = RustcBuilder::all_rustc()?;
    let git2 = Git2Builder::all_git()?;

    Emitter::default()
        .add_instructions(&build)?
        .add_instructions(&cargo)?
        .add_instructions(&rustc)?
        .add_instructions(&git2)?
        .emit()?;

    Ok(())
}
