use miette::IntoDiagnostic;

use crate::{cli::SyncArgs, shell::Shell};

pub async fn sync(shell: &mut Shell, args: SyncArgs) -> miette::Result<()> {
    let cur_dir = args
        .path
        .unwrap_or_else(|| std::env::current_dir().unwrap());

    shell
        .status("Initializing", "TypeScript runtime environment")
        .into_diagnostic()?;

    let divvun_rt_path = cur_dir.join(".divvun-rt");

    // Remove existing .divvun-rt directory
    match std::fs::remove_dir_all(&divvun_rt_path) {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => {
            return Err(miette::miette!(
                "Failed to remove .divvun-rt directory: {}",
                e
            ));
        }
    }

    shell
        .status("Generating", "Divvun Runtime TypeScript bindings")
        .into_diagnostic()?;
    divvun_runtime::ts::generate(&divvun_rt_path).into_diagnostic()?;

    shell
        .status("Checking", "Deno installation")
        .into_diagnostic()?;
    let result = std::process::Command::new("deno")
        .args(&["--version"])
        .output();

    match result {
        Ok(output) if output.status.success() => {
            let version = String::from_utf8_lossy(&output.stdout);
            shell
                .status(
                    "Found",
                    format!("Deno {}", version.lines().next().unwrap_or("")),
                )
                .into_diagnostic()?;
        }
        _ => {
            return Err(miette::miette!(
                "Deno is not installed or not in PATH. Please install Deno from https://deno.land/"
            ));
        }
    }

    Ok(())
}
