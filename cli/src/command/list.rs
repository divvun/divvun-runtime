use divvun_runtime::bundle::Bundle;
use termcolor::Color;

use crate::{cli::ListArgs, shell::Shell};

use super::utils;

pub fn list(shell: &mut Shell, args: ListArgs) -> anyhow::Result<()> {
    let path = args
        .path
        .unwrap_or_else(|| std::env::current_dir().unwrap());

    let bundle = if path.extension().map(|x| x.as_encoded_bytes()) == Some(b"drb") {
        Bundle::metadata_from_bundle(&path)?
    } else {
        // For TypeScript files, check if we need to generate the AST
        let pipeline_path = if path.ends_with(".ts") {
            path.clone()
        } else {
            path.join("pipeline.ts")
        };

        let pipeline_json_path = if path.ends_with(".ts") {
            path.parent().unwrap().join("pipeline.json")
        } else {
            path.join("pipeline.json")
        };

        if pipeline_path.exists() && !pipeline_json_path.exists() {
            shell.status("Generating", "pipeline.json from TypeScript")?;
            utils::prepare_typescript_pipeline(shell, &pipeline_path, false)?;
            crate::deno_rt::save_ast(&path, &pipeline_json_path)?;
        }

        Bundle::metadata_from_path(&path)?
    };

    let pipelines: Vec<&str> = bundle.pipelines.keys().map(|s| s.as_str()).collect();
    let default = &bundle.default;

    shell.status("Bundle", path.display())?;
    shell.status("Pipelines", format!("{} available", pipelines.len()))?;

    for name in pipelines {
        if name == default {
            shell.status_with_color("•", format!("{} (default)", name), Color::Green)?;
        } else {
            shell.status("•", name)?;
        }
    }

    Ok(())
}
