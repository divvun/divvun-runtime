use syntect::easy::HighlightLines;
use syntect::util::LinesWithEndings;

use crate::{get_syntax_and_theme, get_syntax_set, style_to_ansi, ANSI_RESET};

pub fn highlight_to_terminal(content: &str, syntax_name: &str) -> String {
    let Some((syntax, theme)) = get_syntax_and_theme(syntax_name) else {
        return content.to_string();
    };

    let syntax_set = get_syntax_set(syntax_name);
    let mut highlighter = HighlightLines::new(syntax, theme);
    let mut output = String::new();

    for line in LinesWithEndings::from(content) {
        let is_literal_line = syntax_name == "cg3" && line.trim_start().starts_with(':');

        let ranges = highlighter
            .highlight_line(line, syntax_set)
            .unwrap_or_default();

        if is_literal_line {
            output.push_str("\x1b[48;2;60;90;60m");
        }

        for (style, text) in ranges {
            output.push_str(&style_to_ansi(style));
            output.push_str(text);
            output.push_str(ANSI_RESET);
        }

        if is_literal_line {
            output.push_str(ANSI_RESET);
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
