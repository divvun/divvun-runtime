use std::path::PathBuf;

use box_format::{BoxFileWriter, BoxPath, Compression};
use divvun_runtime::ast::PipelineDefinition;
use pyo3::Python;

use crate::{
    cli::BundleArgs,
    py_rt::{init_py, PYTHON},
    shell::Shell,
};

pub fn bundle(shell: &mut Shell, args: BundleArgs) -> anyhow::Result<()> {
    shell.status("Initializing", "Python virtual environment")?;
    init_py();

    let pipeline_path = args
        .pipeline_path
        .unwrap_or_else(|| PathBuf::from("./pipeline.py"));
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
    let value = match crate::py_rt::dump_ast(&pipeline_file) {
        Ok(v) => v,
        Err(e) => {
            match e {
                crate::py_rt::Error::Python(py_err) => {
                    shell.error(format!("Python error while processing pipeline file:\n",))?;
                    Python::with_gil(|py| {
                        py_err.print(py);
                    });
                }
                _ => {
                    unreachable!("Unexpected error while processing pipeline file: {}", e)
                }
            }
            std::process::exit(1);
        }
    };

    let pd: PipelineDefinition = serde_json::from_value(value).unwrap();
    shell.status("Validating", assets_path.display())?;

    let mut missing_assets = false;
    for asset_path in pd.assets().iter() {
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
        &mut std::io::Cursor::new(serde_json::to_vec(&pd)?),
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

    box_file.finish()?;

    Ok(())
}
