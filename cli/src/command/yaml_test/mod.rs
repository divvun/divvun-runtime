mod error_types;
mod markup;
mod parser;
mod sentence;

pub use error_types::ErrorType;
pub use markup::{ErrorContent, ErrorMarkup, ErrorSegment};
pub use parser::{parse_markup, ParseError};
pub use sentence::ErrorAnnotatedSentence;

use crate::cli::YamlTestArgs;
use crate::shell::Shell;

pub async fn yaml_test(_shell: &mut Shell, _args: YamlTestArgs) -> anyhow::Result<()> {
    println!("Hello, world!");
    Ok(())
}
