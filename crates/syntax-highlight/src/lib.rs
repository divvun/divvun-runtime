use once_cell::sync::Lazy;
use syntect::highlighting::{Style, Theme, ThemeSet};
use syntect::parsing::{SyntaxDefinition, SyntaxSet, SyntaxSetBuilder};

#[cfg(feature = "html")]
mod html;
#[cfg(feature = "terminal")]
mod terminal;

#[cfg(feature = "html")]
pub use html::highlight_to_html;
#[cfg(feature = "terminal")]
pub use terminal::{highlight_to_terminal, supports_color};

static CUSTOM_SYNTAX_SET: Lazy<SyntaxSet> = Lazy::new(|| {
    let mut builder = SyntaxSetBuilder::new();
    let cg3_syntax = include_str!("../syntaxes/cg3.sublime-syntax");
    builder.add(
        SyntaxDefinition::load_from_str(cg3_syntax, true, Some("cg3"))
            .expect("Failed to load CG3 syntax"),
    );
    builder.build()
});

static DEFAULT_SYNTAX_SET: Lazy<SyntaxSet> = Lazy::new(SyntaxSet::load_defaults_newlines);

static THEME_SET: Lazy<ThemeSet> = Lazy::new(ThemeSet::load_defaults);

pub fn get_syntax_and_theme(
    syntax_name: &str,
) -> Option<(&'static syntect::parsing::SyntaxReference, &'static Theme)> {
    let syntax = if syntax_name == "cg3" {
        CUSTOM_SYNTAX_SET.find_syntax_by_name("VISL CG3")
    } else {
        DEFAULT_SYNTAX_SET
            .find_syntax_by_extension(syntax_name)
            .or_else(|| DEFAULT_SYNTAX_SET.find_syntax_by_name(syntax_name))
    };

    syntax.map(|s| {
        let theme = THEME_SET
            .themes
            .get("base16-mocha.dark")
            .or_else(|| THEME_SET.themes.get("base16-ocean.dark"))
            .or_else(|| THEME_SET.themes.values().next())
            .expect("No themes available");
        (s, theme)
    })
}

pub fn get_syntax_set(syntax_name: &str) -> &'static SyntaxSet {
    if syntax_name == "cg3" {
        &*CUSTOM_SYNTAX_SET
    } else {
        &*DEFAULT_SYNTAX_SET
    }
}

pub fn style_to_ansi(style: Style) -> String {
    let mut codes = String::new();

    if style
        .font_style
        .contains(syntect::highlighting::FontStyle::BOLD)
    {
        codes.push_str("\x1b[1m");
    }
    if style
        .font_style
        .contains(syntect::highlighting::FontStyle::ITALIC)
    {
        codes.push_str("\x1b[3m");
    }
    if style
        .font_style
        .contains(syntect::highlighting::FontStyle::UNDERLINE)
    {
        codes.push_str("\x1b[4m");
    }

    let fg = style.foreground;
    codes.push_str(&format!("\x1b[38;2;{};{};{}m", fg.r, fg.g, fg.b));

    let bg = style.background;
    codes.push_str(&format!("\x1b[48;2;{};{};{}m", bg.r, bg.g, bg.b));

    codes
}

pub const ANSI_RESET: &str = "\x1b[0m";
