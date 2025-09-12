use std::path::Path;

use crate::shell::Shell;

/// Prepares a TypeScript pipeline for execution by:
/// 1. Generating/updating .divvun-rt directory with TypeScript bindings
/// 2. Running `deno check` on the pipeline file (unless skipped)
pub fn prepare_typescript_pipeline(
    shell: &mut Shell,
    pipeline_path: &Path,
    skip_check: bool,
) -> anyhow::Result<()> {
    // Generate/update .divvun-rt directory with TypeScript bindings
    let divvun_rt_path = pipeline_path
        .parent()
        .unwrap_or_else(|| std::path::Path::new("./"))
        .join(".divvun-rt");

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

    // Type check pipeline file unless skipped
    if !skip_check {
        shell.status("Type-checking", "pipeline with Deno")?;
        let output = std::process::Command::new("deno")
            .args(&["check", pipeline_path.to_str().unwrap()])
            .output();

        match output {
            Ok(result) if result.status.success() => {
                shell.status("Type-check", "passed")?;
            }
            Ok(result) => {
                shell.error("TypeScript type checking failed:")?;
                if !result.stderr.is_empty() {
                    shell.error(String::from_utf8_lossy(&result.stderr))?;
                }
                if !result.stdout.is_empty() {
                    shell.error(String::from_utf8_lossy(&result.stdout))?;
                }
                std::process::exit(1);
            }
            Err(e) => {
                shell.error(format!("Failed to run deno check: {}", e))?;
                shell.error("Make sure Deno is installed and available in PATH")?;
                std::process::exit(1);
            }
        }
    }

    Ok(())
}
