use std::path::PathBuf;

use box_format::{BoxFileWriter, BoxPath, Compression};
use divvun_runtime::ast::PipelineBundle;

use crate::{cli::BundleArgs, shell::Shell};

use super::utils;

pub fn bundle(shell: &mut Shell, args: BundleArgs) -> anyhow::Result<()> {
    shell.status("Initializing", "TypeScript runtime environment")?;

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

    let pipeline_file = match std::fs::read_to_string(&pipeline_path) {
        Ok(v) => v,
        Err(e) => {
            shell.error(format!(
                "Failed to read pipeline file at path {}: {}",
                pipeline_path.display(),
                e
            ))?;
            std::process::exit(1);
        }
    };

    shell.status("Processing", &pipeline_path.display())?;
    let value = match crate::deno_rt::dump_ast(&pipeline_file) {
        Ok(v) => v,
        Err(e) => {
            shell.error(format!("Error while processing pipeline file: {}", e))?;
            std::process::exit(1);
        }
    };

    let mut bundle: PipelineBundle = PipelineBundle::from_json(value).unwrap();

    // Filter out dev pipelines
    let dev_pipelines: Vec<String> = bundle
        .pipelines
        .iter()
        .filter(|(_, p)| p.dev)
        .map(|(name, _)| name.clone())
        .collect();

    if !dev_pipelines.is_empty() {
        shell.status(
            "Skipping",
            format!(
                "{} dev pipeline(s): {}",
                dev_pipelines.len(),
                dev_pipelines.join(", ")
            ),
        )?;
        bundle.pipelines.retain(|_, p| !p.dev);
    }

    if bundle.pipelines.is_empty() {
        shell.error("Cannot create bundle: all pipelines are marked as dev-only")?;
        std::process::exit(1);
    }

    // Update default if it was a dev pipeline
    if !bundle.pipelines.contains_key(&bundle.default) {
        bundle.default = bundle.pipelines.keys().next().unwrap().clone();
    }

    shell.status("Validating", assets_path.display())?;

    let mut missing_assets = false;
    for asset_path in bundle.assets().iter() {
        let full_path = assets_path.join(asset_path);
        if !full_path.exists() {
            shell.error(format!("Asset file not found: {}", full_path.display()))?;
            missing_assets = true;
        }
    }

    if missing_assets {
        shell.error("Some assets are missing. Please check the asset paths.")?;
        std::process::exit(1);
    }

    std::fs::remove_file("./bundle.drb").unwrap_or(());
    let mut box_file = BoxFileWriter::create_with_alignment("./bundle.drb", 8)?;
    box_file.insert(
        Compression::Stored,
        BoxPath::new("pipeline.json").unwrap(),
        &mut std::io::Cursor::new(serde_json::to_vec(&bundle)?),
        Default::default(),
    )?;

    let maybe_assets = match std::fs::read_dir(&assets_path) {
        Ok(v) => Some(v),
        Err(x) if x.kind() == std::io::ErrorKind::NotFound && args.assets_path.is_none() => None,
        Err(e) => {
            shell.error(format!("Failed to read assets directory: {}", e))?;
            std::process::exit(1);
        }
    };

    if let Some(assets) = maybe_assets {
        for entry in assets.filter_map(Result::ok) {
            if !entry.file_type()?.is_file() {
                continue;
            }
            box_file.insert(
                Compression::Stored,
                BoxPath::new(&entry.file_name())?,
                &mut std::fs::File::open(&entry.path())?,
                Default::default(),
            )?;
        }
    }

    // Set bundle metadata attributes
    if let Some(bundle_type) = &args.r#type {
        box_file.set_file_attr("drb.type", bundle_type.as_bytes().to_vec())?;
    }
    if let Some(name) = &args.name {
        box_file.set_file_attr("drb.name", name.as_bytes().to_vec())?;
    }
    if let Some(version) = &args.vers {
        box_file.set_file_attr("drb.version", version.as_bytes().to_vec())?;
    }

    box_file.finish()?;

    Ok(())
}
