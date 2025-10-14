use syntect::easy::HighlightLines;
use syntect::html::{styled_line_to_highlighted_html, IncludeBackground};
use syntect::util::LinesWithEndings;

use crate::{get_syntax_and_theme, get_syntax_set};

pub fn highlight_to_html(content: &str, syntax_name: &str) -> String {
    let Some((syntax, theme)) = get_syntax_and_theme(syntax_name) else {
        return html_escape::encode_text(content).to_string();
    };

    let syntax_set = get_syntax_set(syntax_name);
    let mut highlighter = HighlightLines::new(syntax, theme);
    let mut html = String::new();

    for line in LinesWithEndings::from(content) {
        let is_literal_line = syntax_name == "cg3" && line.trim_start().starts_with(':');

        let ranges = highlighter
            .highlight_line(line, syntax_set)
            .unwrap_or_default();
        let highlighted = styled_line_to_highlighted_html(&ranges, IncludeBackground::No)
            .unwrap_or_else(|_| html_escape::encode_text(line).to_string());

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
