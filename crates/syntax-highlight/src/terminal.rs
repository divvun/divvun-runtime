use syntect::easy::HighlightLines;
use syntect::highlighting::Color;
use syntect::util::LinesWithEndings;

use crate::{get_syntax_and_theme, get_syntax_set, style_to_ansi, style_to_ansi_fg_only};

pub fn highlight_to_terminal(content: &str, syntax_name: &str) -> String {
    highlight_to_terminal_with_theme(content, syntax_name, None, None)
}

pub fn highlight_to_terminal_with_theme(
    content: &str,
    syntax_name: &str,
    theme_name: Option<&str>,
    override_bg: Option<Color>,
) -> String {
    eprintln!("DEBUG terminal: override_bg = {:?}", override_bg);

    let Some((syntax, theme)) = get_syntax_and_theme(syntax_name, theme_name) else {
        return content.to_string();
    };

    let syntax_set = get_syntax_set(syntax_name);
    let mut highlighter = HighlightLines::new(syntax, theme);
    let mut output = String::new();

    // Set global background once at the start if override is provided
    if let Some(bg) = override_bg {
        output.push_str(&format!("\x1b[48;2;{};{};{}m", bg.r, bg.g, bg.b));
    }

    for line in LinesWithEndings::from(content) {
        let is_literal_line = syntax_name == "cg3" && line.trim_start().starts_with(':');

        let ranges = highlighter
            .highlight_line(line, syntax_set)
            .unwrap_or_default();

        if is_literal_line {
            output.push_str("\x1b[48;2;60;90;60m");
        }

        for (style, text) in ranges {
            if override_bg.is_some() && !is_literal_line {
                // Use FG-only when we have global background
                output.push_str(&style_to_ansi_fg_only(style));
            } else {
                // Use full style including BG for special cases
                output.push_str(&style_to_ansi(style));
            }
            output.push_str(text);
        }

        if is_literal_line {
            // Reset to global background after CG3 literal line
            if let Some(bg) = override_bg {
                output.push_str(&format!("\x1b[48;2;{};{};{}m", bg.r, bg.g, bg.b));
            }
        }
    }

    output
}

pub fn supports_color() -> bool {
    std::env::var("NO_COLOR").is_err()
        && (std::env::var("COLORTERM").is_ok()
            || std::env::var("TERM")
                .map(|t| t.contains("color") || t.contains("256") || t.contains("truecolor"))
                .unwrap_or(false))
}
