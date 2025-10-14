use std::process::Command;

use crate::{cli::TestArgs, shell::Shell};

pub async fn test(_shell: &mut Shell, args: TestArgs) -> anyhow::Result<()> {
    let exe_path = std::env::current_exe()?;

    let mut cmd = Command::new("deno");
    cmd.arg("test")
        .arg("--allow-ffi")
        .arg("--allow-env")
        .arg("--no-check")
        .env("LIB_PATH", exe_path);

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
