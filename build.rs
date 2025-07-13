use vergen_git2::{BuildBuilder, CargoBuilder, Emitter, Git2Builder, RustcBuilder};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").expect("CARGO_CFG_TARGET_OS not defined");

    if let Ok(path) = std::env::var("LIBTORCH") {
        println!("cargo:rustc-link-search=native={}/lib", path);
    } else {
        if target_os == "macos" {
            println!("cargo:rustc-link-search=native=/opt/libtorch/lib");
        } else {
            panic!("Unsupported target OS: {}", target_os);
        }
    }

    println!("cargo:rustc-link-lib=c10");
    println!("cargo:rustc-link-lib=torch");
    println!("cargo:rustc-link-lib=torch_cpu");

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
