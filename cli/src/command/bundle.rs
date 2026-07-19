use std::path::{Path, PathBuf};

use box_format::{BoxFileWriter, BoxPath, Compression, CompressionConfig};
use divvun_runtime::ast::PipelineBundle;
use miette::IntoDiagnostic;
use walkdir::WalkDir;

use crate::{cli::BundleArgs, shell::Shell};

use super::utils;

const BUNDLE_ALIGNMENT: u32 = 16;

async fn insert_assets(box_file: &mut BoxFileWriter, assets_path: &Path) -> miette::Result<()> {
    let mut files = WalkDir::new(assets_path)
        .into_iter()
        .map(|entry| entry.into_diagnostic())
        .collect::<miette::Result<Vec<_>>>()?;
    files.sort_by(|a, b| a.path().cmp(b.path()));

    for entry in files
        .into_iter()
        .filter(|entry| entry.file_type().is_file())
    {
        let relative_path = entry.path().strip_prefix(assets_path).into_diagnostic()?;
        let box_path = BoxPath::new(relative_path).into_diagnostic()?;
        if let Some(parent) = box_path.parent() {
            box_file
                .mkdir_all(parent.into_owned(), Default::default())
                .into_diagnostic()?;
        }

        let file = tokio::fs::File::open(entry.path())
            .await
            .into_diagnostic()?;
        let mut reader = tokio::io::BufReader::new(file);
        box_file
            .insert(
                &CompressionConfig::new(Compression::Stored),
                box_path,
                &mut reader,
                Default::default(),
            )
            .await
            .into_diagnostic()?;
    }

    Ok(())
}

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
    let mut box_file = BoxFileWriter::create_with_alignment("./bundle.drb", BUNDLE_ALIGNMENT)
        .await
        .into_diagnostic()?;
    box_file
        .insert(
            &CompressionConfig::new(Compression::Stored),
            BoxPath::new("pipeline.json").into_diagnostic()?,
            &mut std::io::Cursor::new(serde_json::to_vec(&bundle).into_diagnostic()?),
            Default::default(),
        )
        .await
        .into_diagnostic()?;

    let assets_exist = match std::fs::read_dir(&assets_path) {
        Ok(_) => true,
        Err(x) if x.kind() == std::io::ErrorKind::NotFound && args.assets_path.is_none() => false,
        Err(e) => {
            return Err(miette::miette!("Failed to read assets directory: {}", e));
        }
    };

    if assets_exist {
        insert_assets(&mut box_file, &assets_path).await?;
    }

    // Set bundle metadata attributes
    if let Some(bundle_type) = &args.r#type {
        box_file
            .set_file_attr("drb.type", bundle_type.as_bytes().to_vec().into())
            .into_diagnostic()?;
    }
    if let Some(name) = &args.name {
        box_file
            .set_file_attr("drb.name", name.as_bytes().to_vec().into())
            .into_diagnostic()?;
    }
    if let Some(version) = &args.vers {
        box_file
            .set_file_attr("drb.version", version.as_bytes().to_vec().into())
            .into_diagnostic()?;
    }

    box_file.finish().await.into_diagnostic()?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use box_format::BoxFileReader;

    #[tokio::test]
    async fn nested_assets_are_stored_at_sixteen_byte_alignment() {
        let temp = tempfile::tempdir().unwrap();
        let assets = temp.path().join("assets");
        let nested = assets.join("model");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(nested.join("weights.bin"), b"mapped model bytes").unwrap();

        let bundle_path = temp.path().join("bundle.drb");
        let mut writer = BoxFileWriter::create_with_alignment(&bundle_path, BUNDLE_ALIGNMENT)
            .await
            .unwrap();
        insert_assets(&mut writer, &assets).await.unwrap();
        writer.finish().await.unwrap();

        let reader = BoxFileReader::open(&bundle_path).await.unwrap();
        assert_eq!(reader.alignment(), BUNDLE_ALIGNMENT);
        let record = reader
            .find(&BoxPath::new("model/weights.bin").unwrap())
            .unwrap()
            .as_file()
            .unwrap();
        let mapped = reader.memory_map(record).unwrap();
        let bytes = mapped.as_slice().unwrap();
        assert_eq!(&*bytes, b"mapped model bytes");
        assert_eq!((bytes.as_ptr() as usize) % BUNDLE_ALIGNMENT as usize, 0);
    }
}
