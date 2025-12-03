mod error_types;

pub use error_types::ErrorType;

use crate::cli::YamlTestArgs;
use crate::shell::Shell;

pub async fn yaml_test(_shell: &mut Shell, _args: YamlTestArgs) -> anyhow::Result<()> {
    println!("Hello, world!");
    Ok(())
}
