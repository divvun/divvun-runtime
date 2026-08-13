use vergen_gitcl::{Build, Cargo, Emitter, Gitcl, Rustc};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // cg3 and hfst are pure-Rust native ports, so the only C left in the graph
    // is executorch's vendored XNNPACK (plus cpuinfo/pthreadpool), which
    // divvun-speech pulls in under mod-speech.
    link_apple_compiler_rt();

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

/// Link clang's compiler-rt on Apple targets when mod-speech is enabled.
///
/// XNNPACK's pthreadpool is compiled by CMake at a deployment target below the
/// host SDK (minos 11.0 vs 26.x). At that target clang cannot resolve the
/// `__builtin_available` guard in `memory.c` statically, so it emits a runtime
/// call to `___isPlatformVersionAtLeast`. That lives in compiler-rt, which rustc
/// never links because it invokes the linker with `-nodefaultlibs` — leaving the
/// symbol undefined in the final binary.
///
/// Raising the deployment target would also silence it, but minos 11.0 is
/// deliberate for distribution, so pull in compiler-rt instead.
fn link_apple_compiler_rt() {
    if std::env::var_os("CARGO_FEATURE_MOD_SPEECH").is_none() {
        return;
    }

    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let is_sim = std::env::var("CARGO_CFG_TARGET_ABI").as_deref() == Ok("sim");
    let lib = match target_os.as_str() {
        "macos" => "clang_rt.osx",
        "ios" if is_sim => "clang_rt.iossim",
        "ios" => "clang_rt.ios",
        _ => return,
    };

    let Some(dir) = clang_resource_dir().map(|d| d.join("lib").join("darwin")) else {
        println!("cargo:warning=could not determine clang resource dir; skipping {lib}");
        return;
    };

    if !dir.join(format!("lib{lib}.a")).exists() {
        println!("cargo:warning=lib{lib}.a not found in {}", dir.display());
        return;
    }

    println!("cargo:rustc-link-search=native={}", dir.display());
    println!("cargo:rustc-link-lib=static:-bundle={lib}");
}

fn clang_resource_dir() -> Option<std::path::PathBuf> {
    // Prefer xcrun so we get the active toolchain rather than whatever `clang`
    // happens to be first on PATH.
    let candidates: [&[&str]; 2] = [&["xcrun", "clang"], &["clang"]];

    for argv in candidates {
        let Ok(out) = std::process::Command::new(argv[0])
            .args(&argv[1..])
            .arg("-print-resource-dir")
            .output()
        else {
            continue;
        };
        if !out.status.success() {
            continue;
        }
        let Ok(path) = String::from_utf8(out.stdout) else {
            continue;
        };
        let path = std::path::PathBuf::from(path.trim());
        if path.is_dir() {
            return Some(path);
        }
    }

    None
}
