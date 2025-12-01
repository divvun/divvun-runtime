use once_cell::sync::Lazy;
use std::str::FromStr;
use syntect::highlighting::{Color, Highlighter, Style, Theme, ThemeSet};
use syntect::parsing::{SyntaxDefinition, SyntaxSet, SyntaxSetBuilder};

#[cfg(feature = "html")]
mod html;
#[cfg(feature = "terminal")]
mod terminal;

#[cfg(feature = "html")]
pub use html::highlight_to_html;
#[cfg(feature = "terminal")]
pub use terminal::{highlight_to_terminal, highlight_to_terminal_with_theme, supports_color};

// Re-export Color for use in CLI
pub use syntect::highlighting::Color as ThemeColor;

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

pub fn list_available_themes() -> Vec<&'static str> {
    THEME_SET.themes.keys().map(|s| s.as_str()).collect()
}

pub fn get_theme_by_name(theme_name: &str) -> Option<&'static Theme> {
    THEME_SET.themes.get(theme_name)
}

pub fn get_default_theme_for_background(is_dark: bool) -> &'static str {
    if is_dark {
        "base16-mocha.dark"
    } else {
        "base16-ocean.light"
    }
}

pub fn get_syntax_and_theme(
    syntax_name: &str,
    theme_name: Option<&str>,
) -> Option<(&'static syntect::parsing::SyntaxReference, &'static Theme)> {
    let syntax = if syntax_name == "cg3" {
        CUSTOM_SYNTAX_SET.find_syntax_by_name("VISL CG3")
    } else {
        DEFAULT_SYNTAX_SET
            .find_syntax_by_extension(syntax_name)
            .or_else(|| DEFAULT_SYNTAX_SET.find_syntax_by_name(syntax_name))
    };

    syntax.map(|s| {
        let theme = if let Some(name) = theme_name {
            THEME_SET.themes.get(name)
        } else {
            None
        }
        .or_else(|| THEME_SET.themes.get("base16-mocha.dark"))
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

pub fn style_to_ansi_fg_only(style: Style) -> String {
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

    codes
}

pub const ANSI_RESET: &str = "\x1b[0m";

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

fn color_to_ansi_fg(color: Color) -> String {
    format!("\x1b[38;2;{};{};{}m", color.r, color.g, color.b)
}

fn color_to_ansi_bg(color: Color) -> String {
    format!("\x1b[48;2;{};{};{}m", color.r, color.g, color.b)
}

pub fn extract_command_colors(theme: &Theme) -> (CommandColors, Color) {
    let highlighter = Highlighter::new(theme);

    // Helper to get color for a scope, falling back to foreground
    let get_scope_color = |scope_str: &str| -> Color {
        use syntect::parsing::{Scope, ScopeStack};
        if let Ok(scope) = Scope::from_str(scope_str) {
            let mut stack = ScopeStack::new();
            stack.push(scope);
            let style = highlighter.style_for_stack(&stack.as_slice());
            style.foreground
        } else {
            theme.settings.foreground.unwrap_or(Color {
                r: 255,
                g: 255,
                b: 255,
                a: 255,
            })
        }
    };

    let background = theme.settings.background.unwrap_or(Color {
        r: 0,
        g: 0,
        b: 0,
        a: 255,
    });

    let foreground = theme.settings.foreground.unwrap_or(Color {
        r: 255,
        g: 255,
        b: 255,
        a: 255,
    });

    let colors = CommandColors {
        background: color_to_ansi_bg(background),
        foreground: color_to_ansi_fg(foreground),
        module: color_to_ansi_fg(get_scope_color("entity.name.namespace")),
        command: color_to_ansi_fg(get_scope_color("entity.name.function")),
        type_ann: color_to_ansi_fg(get_scope_color("storage.type")),
        string: color_to_ansi_fg(get_scope_color("string")),
        number: color_to_ansi_fg(get_scope_color("constant.numeric")),
        boolean: color_to_ansi_fg(get_scope_color("constant.language")),
        returns: color_to_ansi_fg(get_scope_color("comment")),
    };

    (colors, background)
}
