use std::io::Write;
use std::process::Command;

use crate::{cli::TestArgs, shell::Shell};

const DENO_MOD_TS: &str = include_str!("../../../bindings/deno/mod.ts");

pub async fn test(_shell: &mut Shell, args: TestArgs) -> anyhow::Result<()> {
    let exe_path = std::env::current_exe()?;

    let preload_script = format!(
        "{}\n\nsetLibPath(\"{}\");\n(globalThis as any)[\"Bundle\"] = Bundle;\n",
        DENO_MOD_TS,
        exe_path.display()
    );

    let temp_dir = tempfile::tempdir()?;
    let preload_path = temp_dir.path().join("preload.ts");
    let mut preload_file = std::fs::File::create(&preload_path)?;
    preload_file.write_all(preload_script.as_bytes())?;
    preload_file.flush()?;
    drop(preload_file);

    let mut cmd = Command::new("deno");
    cmd.arg("test")
        .arg("--allow-ffi")
        .arg("--no-check")
        .arg("--preload")
        .arg(&preload_path);

    for file in &args.files {
        cmd.arg(file);
    }

    if !args.script_args.is_empty() {
        cmd.arg("--");
        for arg in &args.script_args {
            cmd.arg(arg);
        }
    }

    let status = cmd.status()?;

    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }

    Ok(())
}
