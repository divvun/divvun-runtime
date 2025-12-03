mod parser;
mod runner;
mod output;

use crate::cli::YamlTestArgs;
use crate::shell::Shell;

pub async fn yaml_test(_shell: &mut Shell, args: YamlTestArgs) -> anyhow::Result<()> {
    println!("Running YAML tests from file: {}", args.yaml_file.display());
    // TODO: Parse YAML file
    // TODO: Run tests through bundle
    // TODO: Compare results
    // TODO: Output formatted results
    Ok(())
}