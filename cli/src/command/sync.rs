use crate::{cli::SyncArgs, shell::Shell};

pub async fn sync(shell: &mut Shell, args: SyncArgs) -> anyhow::Result<()> {
    let cur_dir = args
        .path
        .unwrap_or_else(|| std::env::current_dir().unwrap());

    shell.status("Initializing", "TypeScript runtime environment")?;

    let divvun_rt_path = cur_dir.join(".divvun-rt");

    // Remove existing .divvun-rt directory
    match std::fs::remove_dir_all(&divvun_rt_path) {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => {
            shell.error(format!("Failed to remove .divvun-rt directory: {}", e))?;
            std::process::exit(1);
        }
    }

    shell.status("Generating", "Divvun Runtime TypeScript bindings")?;
    divvun_runtime::ts::generate(&divvun_rt_path)?;

    shell.status("Checking", "Deno installation")?;
    let result = std::process::Command::new("deno")
        .args(&["--version"])
        .output();

    match result {
        Ok(output) if output.status.success() => {
            let version = String::from_utf8_lossy(&output.stdout);
            shell.status(
                "Found",
                format!("Deno {}", version.lines().next().unwrap_or("")),
            )?;
        }
        _ => {
            shell.error(
                "Deno is not installed or not in PATH. Please install Deno from https://deno.land/",
            )?;
            std::process::exit(1);
        }
    }

    Ok(())
}
