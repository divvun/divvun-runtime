use std::env;
use std::path::PathBuf;
use std::process::Command;

use crate::cli::PlaygroundArgs;
use crate::shell::Shell;

pub fn playground(_shell: &mut Shell, args: PlaygroundArgs) -> anyhow::Result<()> {
    let path = args.path.unwrap_or_else(|| env::current_dir().unwrap());

    let playground_path = find_playground_binary()?;

    let mut cmd = Command::new(&playground_path);
    cmd.arg(path.canonicalize()?);

    let status = cmd.status()?;

    if !status.success() {
        anyhow::bail!("Playground exited with status: {}", status);
    }

    Ok(())
}

fn find_playground_binary() -> anyhow::Result<PathBuf> {
    if let Ok(custom_path) = env::var("DRT_PLAYGROUND") {
        let path = PathBuf::from(custom_path);
        if path.exists() {
            return Ok(path);
        }
    }

    #[cfg(target_os = "macos")]
    {
        let app_path = PathBuf::from(
            "/Applications/Divvun Runtime Playground.app/Contents/MacOS/divvun-rt-playground",
        );
        if app_path.exists() {
            return Ok(app_path);
        }
    }

    if let Ok(output) = Command::new("which").arg("divvun-rt-playground").output() {
        if output.status.success() {
            let path_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path_str.is_empty() {
                return Ok(PathBuf::from(path_str));
            }
        }
    }

    anyhow::bail!(
        "Playground not installed. Install from: https://github.com/divvun/divvun-runtime"
    )
}
