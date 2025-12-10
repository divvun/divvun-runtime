use vergen_gitcl::{BuildBuilder, CargoBuilder, Emitter, GitclBuilder, RustcBuilder};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let target = std::env::var("TARGET").unwrap();
    let build_root = std::env::var("BUILD_ROOT").unwrap_or_else(|_| ".".to_string());

    let sysroot = std::fs::canonicalize(format!("{build_root}/.x/sysroot/{target}")).unwrap();

    println!("cargo:rustc-link-search=native={}", sysroot.display());
    println!("cargo:rerun-if-changed={}", sysroot.display());

    if cfg!(windows) {
        println!("cargo:rustc-link-lib=icudt");
        println!("cargo:rustc-link-lib=icuin");
        println!("cargo:rustc-link-lib=icudata");
        println!("cargo:rustc-link-lib=icui18n");
    } else if cfg!(unix) || target.contains("ios") {
        println!("cargo:rustc-link-lib=static=icuuc");
        println!("cargo:rustc-link-lib=static=icuio");
        println!("cargo:rustc-link-lib=static=icudata");
        println!("cargo:rustc-link-lib=static=icui18n");

        // musl needs gcc_eh for C++ exception handling in static libs
        if target.contains("musl") {
            println!("cargo:rustc-link-lib=gcc_eh");
        }
    } else {
        todo!("BAD OS")
    }

    let build = BuildBuilder::all_build()?;
    let cargo = CargoBuilder::all_cargo()?;
    let rustc = RustcBuilder::all_rustc()?;
    let gitcl = GitclBuilder::all_git()?;

    Emitter::default()
        .add_instructions(&build)?
        .add_instructions(&cargo)?
        .add_instructions(&rustc)?
        .add_instructions(&gitcl)?
        .emit()?;

    Ok(())
}
