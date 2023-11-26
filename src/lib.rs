use std::path::Path;

use ast::{from_ast, PipelineDefinition};
use tempfile::TempDir;

pub mod ast;
pub mod modules;

pub async fn run() -> anyhow::Result<()> {
    Ok(())
}

pub struct Bundle {
    temp_dir: TempDir,
    defn: PipelineDefinition,
}

impl Bundle {
    pub async fn run_pipeline(&self, input: String) -> anyhow::Result<String> {
        let result = from_ast(self.defn.ast.clone(), Box::pin(async { Ok(input) }))?.await?;
        Ok(result)
    }
}

pub fn load_bundle<P: AsRef<Path>>(bundle_path: P) -> anyhow::Result<Bundle> {
    let temp_dir = tempfile::tempdir()?;
    let box_file = box_format::BoxFileReader::open(bundle_path)?;
    box_file.extract_all(temp_dir.path())?;

    let txt = std::fs::read_to_string(temp_dir.path().join("ast.json"))?;
    let jd = &mut serde_json::Deserializer::from_str(&txt);
    let defn: PipelineDefinition = serde_path_to_error::deserialize(jd)?;

    Ok(Bundle { temp_dir, defn })
}
