use divvun_runtime::bundle::Bundle;
use termcolor::Color;

use crate::{cli::ListArgs, shell::Shell};

use super::utils;

pub async fn list(shell: &mut Shell, args: ListArgs) -> anyhow::Result<()> {
    let path = args
        .path
        .unwrap_or_else(|| std::env::current_dir().unwrap());

    // Read box file metadata if it's a .drb bundle
    let (bundle_type, bundle_name, bundle_version) =
        if path.extension().map(|x| x.as_encoded_bytes()) == Some(b"drb") {
            let box_file = box_format::BoxFileReader::open(&path).await?;
            let metadata = box_file.metadata();

            let bundle_type = metadata
                .file_attr("drb.type")
                .map(|v| String::from_utf8_lossy(v).to_string());
            let bundle_name = metadata
                .file_attr("drb.name")
                .map(|v| String::from_utf8_lossy(v).to_string());
            let bundle_version = metadata
                .file_attr("drb.version")
                .map(|v| String::from_utf8_lossy(v).to_string());

            (bundle_type, bundle_name, bundle_version)
        } else {
            (None, None, None)
        };

    let bundle = if path.extension().map(|x| x.as_encoded_bytes()) == Some(b"drb") {
        Bundle::metadata_from_bundle(&path).await?
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

        Bundle::metadata_from_path(&path).await?
    };

    let pipelines: Vec<&str> = bundle.pipelines.keys().map(|s| s.as_str()).collect();
    let default = &bundle.default;

    shell.status("Bundle", path.display())?;

    // Display box metadata if available
    if let Some(ref type_str) = bundle_type {
        shell.status("Type", type_str)?;
    }
    if let Some(ref name_str) = bundle_name {
        shell.status("Name", name_str)?;
    }
    if let Some(ref version_str) = bundle_version {
        shell.status("Version", version_str)?;
    }

    shell.status("Pipelines", format!("{} available", pipelines.len()))?;

    for name in pipelines {
        let pipeline = bundle.pipelines.get(name).unwrap();
        let mut tags = Vec::new();

        if pipeline.dev {
            tags.push("dev");
        }
        if name == default {
            tags.push("default");
        }

        let label = if tags.is_empty() {
            name.to_string()
        } else {
            format!("{} ({})", name, tags.join(", "))
        };

        if name == default || pipeline.dev {
            shell.status_with_color(
                "•",
                label,
                if pipeline.dev {
                    Color::Yellow
                } else {
                    Color::Green
                },
            )?;
        } else {
            shell.status("•", label)?;
        }
    }

    Ok(())
}
