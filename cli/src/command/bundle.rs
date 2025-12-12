use std::path::PathBuf;

use box_format::{BoxFileWriter, BoxPath, Compression};
use divvun_runtime::ast::PipelineBundle;
use miette::IntoDiagnostic;

use crate::{cli::BundleArgs, shell::Shell};

use super::utils;

pub async fn bundle(shell: &mut Shell, args: BundleArgs) -> miette::Result<()> {
    shell
        .status("Initializing", "TypeScript runtime environment")
        .into_diagnostic()?;

    let pipeline_path = args
        .pipeline_path
        .unwrap_or_else(|| PathBuf::from("./pipeline.ts"));

    // Prepare TypeScript environment (sync + type check)
    utils::prepare_typescript_pipeline(shell, &pipeline_path, args.skip_check)?;
    let assets_path = args
        .assets_path
        .as_ref()
        .map(|p| p.clone())
        .unwrap_or_else(|| PathBuf::from("./assets"));

    let pipeline_file = std::fs::read_to_string(&pipeline_path).map_err(|e| {
        miette::miette!(
            "Failed to read pipeline file at path {}: {}",
            pipeline_path.display(),
            e
        )
    })?;

    shell
        .status("Processing", &pipeline_path.display())
        .into_diagnostic()?;
    let value = crate::deno_rt::dump_ast(&pipeline_file)
        .map_err(|e| miette::miette!("Error while processing pipeline file: {}", e))?;

    let mut bundle: PipelineBundle = PipelineBundle::from_json(value).into_diagnostic()?;

    // Filter out dev pipelines
    let dev_pipelines: Vec<String> = bundle
        .pipelines
        .iter()
        .filter(|(_, p)| p.dev)
        .map(|(name, _)| name.clone())
        .collect();

    if !dev_pipelines.is_empty() {
        shell
            .status(
                "Skipping",
                format!(
                    "{} dev pipeline(s): {} (dev pipelines use local @ paths and cannot be bundled)",
                    dev_pipelines.len(),
                    dev_pipelines.join(", ")
                ),
            )
            .into_diagnostic()?;
        bundle.pipelines.retain(|_, p| !p.dev);
    }

    if bundle.pipelines.is_empty() {
        miette::bail!("Cannot create bundle: all pipelines are marked as dev-only");
    }

    // Update default if it was a dev pipeline
    if !bundle.pipelines.contains_key(&bundle.default) {
        bundle.default = bundle.pipelines.keys().next().unwrap().clone();
    }

    shell
        .status("Validating", assets_path.display())
        .into_diagnostic()?;

    let mut missing_assets = Vec::new();
    for asset_path in bundle.assets().iter() {
        let full_path = assets_path.join(asset_path);
        if !full_path.exists() {
            missing_assets.push(full_path.display().to_string());
        }
    }

    if !missing_assets.is_empty() {
        miette::bail!(
            "Missing assets: {}. Please check the asset paths.",
            missing_assets.join(", ")
        );
    }

    std::fs::remove_file("./bundle.drb").unwrap_or(());
    let mut box_file = BoxFileWriter::create_with_alignment("./bundle.drb", 8)
        .await
        .into_diagnostic()?;
    box_file
        .insert(
            Compression::Stored,
            BoxPath::new("pipeline.json").into_diagnostic()?,
            &mut std::io::Cursor::new(serde_json::to_vec(&bundle).into_diagnostic()?),
            Default::default(),
        )
        .await
        .into_diagnostic()?;

    let maybe_assets = match std::fs::read_dir(&assets_path) {
        Ok(v) => Some(v),
        Err(x) if x.kind() == std::io::ErrorKind::NotFound && args.assets_path.is_none() => None,
        Err(e) => {
            return Err(miette::miette!("Failed to read assets directory: {}", e));
        }
    };

    if let Some(assets) = maybe_assets {
        for entry in assets.filter_map(Result::ok) {
            if !entry.file_type().into_diagnostic()?.is_file() {
                continue;
            }
            let file = tokio::fs::File::open(&entry.path())
                .await
                .into_diagnostic()?;
            let mut reader = tokio::io::BufReader::new(file);
            box_file
                .insert(
                    Compression::Stored,
                    BoxPath::new(&entry.file_name()).into_diagnostic()?,
                    &mut reader,
                    Default::default(),
                )
                .await
                .into_diagnostic()?;
        }
    }

    // Set bundle metadata attributes
    if let Some(bundle_type) = &args.r#type {
        box_file
            .set_file_attr("drb.type", bundle_type.as_bytes().to_vec())
            .into_diagnostic()?;
    }
    if let Some(name) = &args.name {
        box_file
            .set_file_attr("drb.name", name.as_bytes().to_vec())
            .into_diagnostic()?;
    }
    if let Some(version) = &args.vers {
        box_file
            .set_file_attr("drb.version", version.as_bytes().to_vec())
            .into_diagnostic()?;
    }

    box_file.finish().await.into_diagnostic()?;

    Ok(())
}
