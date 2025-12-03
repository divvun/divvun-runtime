mod error_types;
mod markup;

pub use error_types::ErrorType;
pub use markup::{ErrorContent, ErrorMarkup, ErrorSegment};

use crate::cli::YamlTestArgs;
use crate::shell::Shell;

pub async fn yaml_test(_shell: &mut Shell, _args: YamlTestArgs) -> anyhow::Result<()> {
    println!("Hello, world!");
    Ok(())
}
