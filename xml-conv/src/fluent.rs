use crate::{Default, Error, ErrorDocument, Id};
use anyhow::Result;
use serde_json::{Map, Value};
use std::collections::HashSet;
use std::fs;
use std::path::Path;

pub fn write_fluent_files(doc: &ErrorDocument, output_dir: &Path) -> Result<()> {
    // Collect all languages from the document
    let mut languages = HashSet::new();

    // Collect languages from defaults
    for default in &doc.defaults {
        for lang in default.header.titles.keys() {
            languages.insert(lang.clone());
        }
        for lang in default.body.descriptions.keys() {
            languages.insert(lang.clone());
        }
    }

    // Collect languages from errors
    for error in &doc.errors {
        for lang in error.header.titles.keys() {
            languages.insert(lang.clone());
        }
        for lang in error.body.descriptions.keys() {
            languages.insert(lang.clone());
        }
    }

    // Create output directory if it doesn't exist
    fs::create_dir_all(output_dir)?;

    // Generate a file for each language
    for lang in languages {
        let fluent_content = to_fluent(doc, &lang);
        let filename = format!("errors-{}.ftl", lang);
        let filepath = output_dir.join(filename);
        fs::write(filepath, fluent_content)?;
    }

    // Generate errors.json metadata file
    let errors_metadata = generate_errors_metadata(doc)?;
    let errors_json_path = output_dir.join("errors.json");
    fs::write(errors_json_path, errors_metadata)?;

    Ok(())
}

pub fn to_fluent(doc: &ErrorDocument, lang: &str) -> String {
    let mut output = String::new();

    // Add header comment
    output.push_str(&format!("# Error messages for language: {}\n", lang));
    output.push_str("# Generated from XML by xml-conv\n\n");

    // Process defaults
    for default in &doc.defaults {
        output.push_str(&format_default(default, lang));
        output.push('\n');
    }

    // Process errors
    for error in &doc.errors {
        output.push_str(&format_error(error, lang));
        output.push('\n');
    }

    output
}

fn format_default(default: &Default, lang: &str) -> String {
    let mut output = String::new();

    // Add comment showing all patterns/IDs
    output.push_str("## Default patterns: ");
    let patterns: Vec<String> = default
        .ids
        .iter()
        .map(|id| match id {
            Id::Regex { pattern } => format!("re:{}", pattern),
            Id::Explicit { value } => format!("id:{}", value),
        })
        .collect();
    output.push_str(&patterns.join(", "));
    output.push('\n');

    // Get title and description for this language
    let missing_title = format!("[Missing title for {}]", lang);
    let missing_desc = format!("[Missing description for {}]", lang);
    let title = default.header.titles.get(lang).unwrap_or(&missing_title);
    let desc = default.body.descriptions.get(lang).unwrap_or(&missing_desc);

    // Use the stable kebab-case ID
    output.push_str(&format!("{} = {}\n", default.id, title));
    output.push_str(&format!("    .desc = {}\n", convert_variables(desc)));
    add_examples_and_refs(&mut output, &default.body.examples, None, lang);

    output
}

fn format_error(error: &Error, lang: &str) -> String {
    let mut output = String::new();

    // Use error ID directly as message ID
    let message_id = &error.id;

    // Get title and description for this language
    let missing_title = format!("[Missing title for {}]", lang);
    let missing_desc = format!("[Missing description for {}]", lang);
    let title = error.header.titles.get(lang).unwrap_or(&missing_title);
    let desc = error.body.descriptions.get(lang).unwrap_or(&missing_desc);

    // Format the message
    output.push_str(&format!("{} = {}\n", message_id, title));
    output.push_str(&format!("    .desc = {}\n", convert_variables(desc)));

    // Add examples and references
    add_examples_and_refs(
        &mut output,
        &error.body.examples,
        error.header.references.as_ref(),
        lang,
    );

    output
}

fn add_examples_and_refs(
    output: &mut String,
    examples: &Option<Vec<crate::Example>>,
    references: Option<&Vec<crate::Reference>>,
    lang: &str,
) {
    // Add examples if any
    if let Some(examples) = examples {
        for (i, example) in examples.iter().enumerate() {
            if example.lang == lang {
                output.push_str(&format!("    .example-{} = {}\n", i + 1, example.text));
            }
        }
    }

    // Add references if any
    if let Some(references) = references {
        for (i, reference) in references.iter().enumerate() {
            output.push_str(&format!("    .ref-{} = {}\n", i + 1, reference.n));
        }
    }
}

fn convert_variables(text: &str) -> String {
    // Convert $1, $2, etc. to {$1}, {$2}, etc. for Fluent
    let mut result = text.to_string();

    // Replace quoted variables like "$1" with "{$1}"
    for i in 1..=9 {
        let old_pattern = format!("\"${}\"", i);
        let new_pattern = format!("{{${}}}", i);
        result = result.replace(&old_pattern, &new_pattern);
    }

    // Replace unquoted variables like $1 with {$1}
    for i in 1..=9 {
        let old_pattern = format!("${}", i);
        let new_pattern = format!("{{${}}}", i);
        // Only replace if not already in braces
        if !result.contains(&format!("{{${}}}", i)) {
            result = result.replace(&old_pattern, &new_pattern);
        }
    }

    result
}

pub fn generate_errors_metadata(doc: &ErrorDocument) -> Result<String> {
    let mut metadata = Map::new();

    // Add defaults with their matching patterns
    for default in &doc.defaults {
        let mut patterns = Vec::new();

        for id in &default.ids {
            match id {
                Id::Regex { pattern } => {
                    let mut pattern_obj = Map::new();
                    pattern_obj.insert("re".to_string(), Value::String(pattern.clone()));
                    patterns.push(Value::Object(pattern_obj));
                }
                Id::Explicit { value } => {
                    let mut id_obj = Map::new();
                    id_obj.insert("id".to_string(), Value::String(value.clone()));
                    patterns.push(Value::Object(id_obj));
                }
            }
        }

        metadata.insert(default.id.clone(), Value::Array(patterns));
    }

    // Add ordinary errors
    for error in &doc.errors {
        let mut id_obj = Map::new();
        id_obj.insert("id".to_string(), Value::String(error.id.clone()));
        let patterns = vec![Value::Object(id_obj)];
        metadata.insert(error.id.clone(), Value::Array(patterns));
    }

    // Convert to pretty-printed JSON
    let json_value = Value::Object(metadata);
    Ok(serde_json::to_string_pretty(&json_value)?)
}
