use once_cell::sync::Lazy;
use syntect::easy::HighlightLines;
use syntect::highlighting::{Style, ThemeSet};
use syntect::html::{styled_line_to_highlighted_html, IncludeBackground};
use syntect::parsing::{SyntaxDefinition, SyntaxSet, SyntaxSetBuilder};
use syntect::util::LinesWithEndings;

// Custom syntax set with CG3
static CUSTOM_SYNTAX_SET: Lazy<SyntaxSet> = Lazy::new(|| {
    let mut builder = SyntaxSetBuilder::new();
    let cg3_syntax = include_str!("../syntaxes/cg3.sublime-syntax");
    builder.add(
        SyntaxDefinition::load_from_str(cg3_syntax, true, Some("cg3"))
            .expect("Failed to load CG3 syntax"),
    );
    builder.build()
});

// Default syntax set (includes JSON, etc.)
static DEFAULT_SYNTAX_SET: Lazy<SyntaxSet> = Lazy::new(SyntaxSet::load_defaults_newlines);

static THEME_SET: Lazy<ThemeSet> = Lazy::new(ThemeSet::load_defaults);

pub fn highlight_to_html(content: &str, syntax_name: &str) -> String {
    // Try custom syntax set first (for CG3)
    let syntax = if syntax_name == "cg3" {
        CUSTOM_SYNTAX_SET.find_syntax_by_name("VISL CG3")
    } else {
        // Try default syntax set for everything else
        DEFAULT_SYNTAX_SET
            .find_syntax_by_extension(syntax_name)
            .or_else(|| DEFAULT_SYNTAX_SET.find_syntax_by_name(syntax_name))
    };

    let Some(syntax) = syntax else {
        // Return plain HTML-escaped text
        return html_escape::encode_text(content).to_string();
    };

    // Available themes: base16-ocean.dark, base16-eighties.dark, base16-mocha.dark, base16-ocean.light,
    // InspiredGitHub, Solarized (dark), Solarized (light), and others
    // Use a darker theme with better contrast
    let theme = THEME_SET
        .themes
        .get("base16-mocha.dark")
        .or_else(|| THEME_SET.themes.get("base16-ocean.dark"))
        .or_else(|| THEME_SET.themes.values().next())
        .expect("No themes available");

    // Use the appropriate syntax set for highlighting
    let syntax_set = if syntax_name == "cg3" {
        &*CUSTOM_SYNTAX_SET
    } else {
        &*DEFAULT_SYNTAX_SET
    };

    let mut highlighter = HighlightLines::new(syntax, theme);
    let mut html = String::new();

    for line in LinesWithEndings::from(content) {
        let is_literal_line = syntax_name == "cg3" && line.trim_start().starts_with(':');

        let ranges: Vec<(Style, &str)> = highlighter
            .highlight_line(line, syntax_set)
            .unwrap_or_default();
        let highlighted = styled_line_to_highlighted_html(&ranges, IncludeBackground::No)
            .unwrap_or_else(|_| html_escape::encode_text(line).to_string());

        // Add background highlight for CG3 literal lines (starting with :)
        if is_literal_line {
            html.push_str("<span style=\"background-color: rgba(100, 150, 100, 0.2);\">");
            html.push_str(&highlighted);
            html.push_str("</span>");
        } else {
            html.push_str(&highlighted);
        }
    }

    html
}
