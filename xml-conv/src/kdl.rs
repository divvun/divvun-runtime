use crate::{Default, Error, ErrorDocument, Id};
use anyhow::Result;

/// Convert an ErrorDocument to KDL format
pub fn to_kdl(doc: &ErrorDocument) -> Result<String> {
    let mut output = String::new();

    // Process defaults (errors with multiple matchers)
    for default in &doc.defaults {
        output.push_str(&format_default(default));
        output.push('\n');
    }

    // Process regular errors
    for error in &doc.errors {
        output.push_str(&format_error(error));
        output.push('\n');
    }

    Ok(output)
}

fn format_default(default: &Default) -> String {
    let mut lines = Vec::new();

    lines.push(format!("error {:?} {{", default.id));
    lines.push("    match {".to_string());

    for id in &default.ids {
        match id {
            Id::Regex { pattern } => {
                lines.push(format!("        re {:?}", pattern));
            }
            Id::Explicit { value } => {
                lines.push(format!("        id {:?}", value));
            }
        }
    }

    lines.push("    }".to_string());
    lines.push("}".to_string());

    lines.join("\n")
}

fn format_error(error: &Error) -> String {
    // Simple errors: if original_id matches the generated id pattern,
    // we can use implicit matching (no match block needed)
    let simple_id = format!("err-{}", error.original_id.to_lowercase().replace(' ', "-"));
    let needs_explicit_match = error.id != simple_id;

    if needs_explicit_match {
        let mut lines = Vec::new();
        lines.push(format!("error {:?} {{", error.id));
        lines.push("    match {".to_string());
        lines.push(format!("        id {:?}", error.original_id));
        lines.push("    }".to_string());
        lines.push("}".to_string());
        lines.join("\n")
    } else {
        // Simple case: implicit matching
        format!("error {:?}", error.id)
    }
}

/// Convert variables from $1, $2 format to ${input} format
pub fn convert_variables_to_input(text: &str) -> String {
    let mut result = text.to_string();

    // Replace $1 with ${input} (the primary input)
    result = result.replace("$1", "${input}");

    // Replace $2, $3, etc. with ${arg2}, ${arg3}, etc.
    for i in 2..=9 {
        let old_pattern = format!("${}", i);
        let new_pattern = format!("${{arg{}}}", i);
        result = result.replace(&old_pattern, &new_pattern);
    }

    // Also handle {$1} -> {$input} for Fluent format
    result = result.replace("{$1}", "{$input}");
    for i in 2..=9 {
        let old_pattern = format!("{{${}}}", i);
        let new_pattern = format!("{{$arg{}}}", i);
        result = result.replace(&old_pattern, &new_pattern);
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_convert_variables() {
        assert_eq!(convert_variables_to_input("$1"), "${input}");
        assert_eq!(convert_variables_to_input("$2"), "${arg2}");
        assert_eq!(
            convert_variables_to_input("The word $1 follows $2"),
            "The word ${input} follows ${arg2}"
        );
        assert_eq!(convert_variables_to_input("{$1}"), "{$input}");
    }
}
