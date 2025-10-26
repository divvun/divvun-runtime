use vergen_git2::{BuildBuilder, CargoBuilder, Emitter, Git2Builder, RustcBuilder};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let target = std::env::var("TARGET").unwrap();

    println!("cargo:rustc-link-search=native=.x/sysroot/{target}/lib");

    if cfg!(windows) {
        println!("cargo:rustc-link-lib=icudt");
        println!("cargo:rustc-link-lib=icuin");
        println!("cargo:rustc-link-lib=icudata");
        println!("cargo:rustc-link-lib=icui18n");
    } else if cfg!(unix) {
        println!("cargo:rustc-link-lib=static=icuuc");
        println!("cargo:rustc-link-lib=static=icuio");
        println!("cargo:rustc-link-lib=static=icudata");
        println!("cargo:rustc-link-lib=static=icui18n");
    } else {
        todo!("BAD OS")
    }

    // const FEATURE_SPEECH: &str = "CARGO_FEATURE_MOD_SPEECH";
    // if std::env::var(FEATURE_SPEECH).ok().as_deref() == Some("1") {
    //     println!("cargo:rustc-link-lib=static=pthreadpool");
    //     println!("cargo:rustc-link-lib=static=cpuinfo");
    //     // static:+whole-archive=mylib
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
