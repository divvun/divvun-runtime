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
    let Some((syntax, theme)) = get_syntax_and_theme(syntax_name, theme_name) else {
        return content.to_string();
    };

    let syntax_set = get_syntax_set(syntax_name);
    let mut highlighter = HighlightLines::new(syntax, theme);
    let mut output = String::new();

    for line in LinesWithEndings::from(content) {
        let is_literal_line = syntax_name == "cg3" && line.trim_start().starts_with(':');
        let has_newline = line.ends_with('\n');

        // Determine background for this line
        let line_bg = if is_literal_line {
            Some(Color {
                r: 60,
                g: 90,
                b: 60,
                a: 255,
            })
        } else {
            override_bg
        };

        // Apply background at start of line
        if let Some(bg) = line_bg {
            output.push_str(&format!("\x1b[48;2;{};{};{}m", bg.r, bg.g, bg.b));
        }

        // Highlight tokens
        let ranges = highlighter
            .highlight_line(line, syntax_set)
            .unwrap_or_default();

        for (style, text) in ranges {
            // Strip trailing newline from text - we handle it separately
            let text = text.trim_end_matches('\n');
            if text.is_empty() {
                continue;
            }

            if line_bg.is_some() {
                // Use FG-only when we have a background set
                output.push_str(&style_to_ansi_fg_only(style));
            } else {
                output.push_str(&style_to_ansi(style));
            }
            output.push_str(text);
        }

        // Fill to end of line with background
        if line_bg.is_some() {
            output.push_str("\x1b[K");
        }

        if has_newline {
            output.push('\n');
        }
    }

    // Final reset
    output.push_str("\x1b[0m");
    output
}

pub fn supports_color() -> bool {
    // Check for explicit disabling
    if std::env::var("NO_COLOR").is_ok() {
        return false;
    }

    // Force color mode
    if std::env::var("FORCE_COLOR").is_ok() {
        return true;
    }

    // Check COLORTERM for true color support
    if let Ok(ct) = std::env::var("COLORTERM") {
        if ct == "truecolor" || ct == "24bit" {
            return true;
        }
    }

    // Check TERM for color capability
    if let Ok(term) = std::env::var("TERM") {
        let term = term.to_lowercase();
        if term.contains("color")
            || term.contains("256")
            || term.contains("xterm")
            || term.contains("screen")
            || term.contains("tmux")
            || term.contains("ansi")
        {
            return true;
        }
    }

    // Windows Terminal
    if std::env::var("WT_SESSION").is_ok() {
        return true;
    }

    // macOS Terminal.app, iTerm2, VS Code, etc.
    std::env::var("TERM_PROGRAM").is_ok()
}
