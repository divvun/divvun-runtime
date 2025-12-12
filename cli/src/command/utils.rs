use std::path::Path;

use miette::IntoDiagnostic;

use crate::shell::Shell;

/// Prepares a TypeScript pipeline for execution by:
/// 1. Generating/updating .divvun-rt directory with TypeScript bindings
/// 2. Running `deno check` on the pipeline file (unless skipped)
pub fn prepare_typescript_pipeline(
    shell: &mut Shell,
    pipeline_path: &Path,
    skip_check: bool,
) -> miette::Result<()> {
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

    // Type check pipeline file unless skipped
    if !skip_check {
        shell
            .status("Type-checking", "pipeline with Deno")
            .into_diagnostic()?;
        let output = std::process::Command::new("deno")
            .args(&["check", pipeline_path.to_str().unwrap()])
            .output();

        match output {
            Ok(result) if result.status.success() => {
                shell.status("Type-check", "passed").into_diagnostic()?;
            }
            Ok(result) => {
                let mut msg = String::from("TypeScript type checking failed");
                if !result.stderr.is_empty() {
                    msg.push_str(&format!("\n{}", String::from_utf8_lossy(&result.stderr)));
                }
                if !result.stdout.is_empty() {
                    msg.push_str(&format!("\n{}", String::from_utf8_lossy(&result.stdout)));
                }
                return Err(miette::miette!("{}", msg));
            }
            Err(e) => {
                return Err(miette::miette!(
                    "Failed to run deno check: {}. Make sure Deno is installed and available in PATH",
                    e
                ));
            }
        }
    }

    Ok(())
}
