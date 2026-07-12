use vergen_gitcl::{Build, Cargo, Emitter, Gitcl, Rustc};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let target = std::env::var("TARGET").unwrap();
    let build_root = std::env::var("BUILD_ROOT").unwrap_or_else(|_| ".".to_string());

    let sysroot = std::fs::canonicalize(format!("{build_root}/.x/sysroot/{target}")).unwrap();

    // Kept for other native deps (e.g. divvun-speech / executorch) that link
    // out of the sysroot. The cg3 and hfst dependencies are now pure-Rust
    // native ports, so the previous ICU/C++ link directives (Windows
    // `/INCLUDE:icudt77_dat`, musl `gcc_eh`) are no longer needed and have been
    // removed — nothing links ICU or C++ from here anymore.
    println!("cargo:rustc-link-search=native={}", sysroot.display());
    println!("cargo:rerun-if-changed={}", sysroot.display());

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
