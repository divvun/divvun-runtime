#[cfg(feature = "highlight")]
mod highlight;
#[cfg(feature = "html")]
mod html;
#[cfg(feature = "terminal")]
mod terminal;

#[cfg(feature = "highlight")]
pub use highlight::*;
#[cfg(feature = "html")]
pub use html::highlight_to_html;
#[cfg(feature = "terminal")]
pub use terminal::{highlight_to_terminal, highlight_to_terminal_with_theme, supports_color};

/// ANSI escapes for each part of a pipeline command as the runtime prints it.
/// `extract_command_colors` takes them from a theme.
#[derive(Clone, Debug)]
pub struct CommandColors {
    pub background: String,
    pub foreground: String,
    pub module: String,
    pub command: String,
    pub type_ann: String,
    pub string: String,
    pub number: String,
    pub boolean: String,
    pub returns: String,
}
