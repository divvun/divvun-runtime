use vergen_gitcl::{Build, Cargo, Emitter, Gitcl, Rustc};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let target = std::env::var("TARGET").unwrap();
    let build_root = std::env::var("BUILD_ROOT").unwrap_or_else(|_| ".".to_string());

    let sysroot = std::fs::canonicalize(format!("{build_root}/.x/sysroot/{target}")).unwrap();

    println!("cargo:rustc-link-search=native={}", sysroot.display());
    println!("cargo:rerun-if-changed={}", sysroot.display());

    // ICU linking is handled by the cg3-rs dependency (the native hfst port is pure Rust)
    // On Windows with static ICU, force-include the ICU data symbol so it isn't stripped
    if target.contains("windows") {
        println!("cargo:rustc-link-arg=/INCLUDE:icudt77_dat");
    }

    // musl needs gcc_eh for C++ exception handling in static libs
    if target.contains("musl") {
        println!("cargo:rustc-link-lib=gcc_eh");
    }

    let build = Build::all_build();
    let cargo = Cargo::all_cargo();
    let rustc = Rustc::all_rustc();
    let gitcl = Gitcl::all_git();

    Emitter::default()
        .add_instructions(&build)?
        .add_instructions(&cargo)?
        .add_instructions(&rustc)?
        .add_instructions(&gitcl)?
        .emit()?;

    Ok(())
}
