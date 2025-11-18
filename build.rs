use vergen_git2::{BuildBuilder, CargoBuilder, Emitter, Git2Builder, RustcBuilder};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let target = std::env::var("TARGET").unwrap();

    let sysroot = std::fs::canonicalize(format!(".x/sysroot/{target}")).unwrap();

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
    } else {
        todo!("BAD OS")
    }

    // if target.contains("ios") {
    //     // Iterate the sysroot lib directory and link all absl static libs
    //     let lib_dir = sysroot.join("lib");
    //     for entry in std::fs::read_dir(lib_dir)? {
    //         let entry = entry?;
    //         let path = entry.path();
    //         if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
    //             if file_name.starts_with("libabsl_") && file_name.ends_with(".a") {
    //                 let lib_name = &file_name[4..file_name.len() - 2]; // Strip "lib" prefix and ".a" suffix
    //                 println!("cargo:rustc-link-lib=static={}", lib_name);
    //             }
    //         }
    //     }
    // }

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
