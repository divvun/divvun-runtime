use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use xmlem::{Document, Element, Node};

pub mod fluent;
pub mod kdl;

#[derive(Debug, Serialize, Deserialize)]
pub struct ErrorDocument {
    pub defaults: Vec<Default>,
    pub errors: Vec<Error>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Default {
    pub id: String,
    pub original_title: String,
    pub ids: Vec<Id>,
    pub header: Header,
    pub body: Body,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Error {
    pub id: String,
    pub original_id: String,
    pub header: Header,
    pub body: Body,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Id {
    Regex { pattern: String },
    Explicit { value: String },
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Header {
    pub titles: HashMap<String, String>,
    pub references: Option<Vec<Reference>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Body {
    pub descriptions: HashMap<String, String>,
    pub examples: Option<Vec<Example>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Reference {
    pub n: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Example {
    pub lang: String,
    pub text: String,
}

pub fn parse_xml_to_errors(xml_content: &str) -> Result<ErrorDocument> {
    let doc: Document = xml_content.parse()?;
    let root = doc.root();

    if root.name(&doc) != "errors" {
        return Err(anyhow!(
            "Expected root element 'errors', found '{}'",
            root.name(&doc)
        ));
    }

    let mut defaults = Vec::new();
    let mut errors = Vec::new();

    for child in root.children(&doc) {
        match child.name(&doc) {
            "defaults" => {
                for default_child in child.children(&doc) {
                    if default_child.name(&doc) == "default" {
                        defaults.push(parse_default(&default_child, &doc)?);
                    }
                }
            }
            "error" => {
                errors.push(parse_error(&child, &doc)?);
            }
            _ => {} // Skip other elements
        }
    }

    Ok(ErrorDocument { defaults, errors })
}

pub fn encode_unicode_identifier(s: &str) -> String {
    let mut result = String::new();

    for ch in s.chars() {
        match ch {
            // Keep ASCII alphanumeric (lowercase)
            'a'..='z' => result.push(ch),
            'A'..='Z' => result.push(ch.to_ascii_lowercase()),
            '0'..='9' => result.push(ch),
            // Keep hyphens
            '-' => result.push(ch),
            // Convert spaces to hyphens
            ' ' => result.push('-'),
            // Convert ASCII punctuation to underscores
            c if c.is_ascii_punctuation() => result.push('_'),
            // Encode non-ASCII characters
            c => {
                let code_point = c as u32;
                if code_point <= 0xFFFF {
                    result.push_str(&format!("_u{:04X}", code_point));
                } else {
                    result.push_str(&format!("_U{:06X}", code_point));
                }
            }
        }
    }

    result
}

pub fn decode_unicode_identifier(s: &str) -> String {
    let mut result = String::new();
    let mut chars = s.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '_' {
            // Check if this is a Unicode escape sequence
            match chars.peek() {
                Some(&'u') => {
                    // Try to parse _uXXXX (4 hex digits)
                    chars.next(); // consume 'u'
                    let hex: String = chars.by_ref().take(4).collect();

                    if hex.len() == 4 {
                        if let Ok(code_point) = u32::from_str_radix(&hex, 16) {
                            if let Some(unicode_char) = char::from_u32(code_point) {
                                result.push(unicode_char);
                                continue;
                            }
                        }
                    }

                    // Failed to decode, restore the characters
                    result.push('_');
                    result.push('u');
                    result.push_str(&hex);
                }
                Some(&'U') => {
                    // Try to parse _UXXXXXX (6 hex digits)
                    chars.next(); // consume 'U'
                    let hex: String = chars.by_ref().take(6).collect();

                    if hex.len() == 6 {
                        if let Ok(code_point) = u32::from_str_radix(&hex, 16) {
                            if let Some(unicode_char) = char::from_u32(code_point) {
                                result.push(unicode_char);
                                continue;
                            }
                        }
                    }

                    // Failed to decode, restore the characters
                    result.push('_');
                    result.push('U');
                    result.push_str(&hex);
                }
                _ => {
                    // Regular underscore
                    result.push('_');
                }
            }
        } else {
            result.push(ch);
        }
    }

    result
}

fn parse_default(element: &Element, doc: &Document) -> Result<Default> {
    let mut ids = Vec::new();
    let mut header = None;
    let mut body = None;

    for child in element.children(doc) {
        match child.name(doc) {
            "ids" => {
                ids = parse_ids(&child, doc)?;
            }
            "header" => {
                header = Some(parse_header(&child, doc)?);
            }
            "body" => {
                body = Some(parse_body(&child, doc)?);
            }
            _ => {}
        }
    }

    let header = header.ok_or_else(|| anyhow!("Missing header in default"))?;
    let body = body.ok_or_else(|| anyhow!("Missing body in default"))?;

    // Generate encoded ID from English title
    let original_title = header
        .titles
        .get("en")
        .cloned()
        .unwrap_or_else(|| "unknown-default".to_string());
    let id = format!("err-{}", encode_unicode_identifier(&original_title));

    Ok(Default {
        id,
        original_title,
        ids,
        header,
        body,
    })
}

fn parse_error(element: &Element, doc: &Document) -> Result<Error> {
    let original_id = element
        .attribute(doc, "id")
        .ok_or_else(|| anyhow!("Error missing id attribute"))?
        .to_string();
    let id = format!("err-{}", encode_unicode_identifier(&original_id));

    let mut header = None;
    let mut body = None;

    for child in element.children(doc) {
        match child.name(doc) {
            "header" => {
                header = Some(parse_header(&child, doc)?);
            }
            "body" => {
                body = Some(parse_body(&child, doc)?);
            }
            _ => {}
        }
    }

    Ok(Error {
        id,
        original_id,
        header: header.ok_or_else(|| anyhow!("Missing header in error"))?,
        body: body.ok_or_else(|| anyhow!("Missing body in error"))?,
    })
}

fn parse_ids(element: &Element, doc: &Document) -> Result<Vec<Id>> {
    let mut ids = Vec::new();

    for child in element.children(doc) {
        match child.name(doc) {
            "re" => {
                if let Some(pattern) = child.attribute(doc, "v") {
                    ids.push(Id::Regex {
                        pattern: pattern.to_string(),
                    });
                }
            }
            "e" => {
                if let Some(value) = child.attribute(doc, "id") {
                    ids.push(Id::Explicit {
                        value: value.to_string(),
                    });
                }
            }
            _ => {}
        }
    }

    Ok(ids)
}

fn parse_header(element: &Element, doc: &Document) -> Result<Header> {
    let mut titles = HashMap::new();
    let mut references = None;

    for child in element.children(doc) {
        match child.name(doc) {
            "title" => {
                let lang = child
                    .attribute(doc, "xml:lang")
                    .or_else(|| child.attribute(doc, "lang"))
                    .unwrap_or("en")
                    .to_string();
                let text = get_element_text(&child, doc);
                titles.insert(lang, text);
            }
            "references" => {
                references = Some(parse_references(&child, doc)?);
            }
            _ => {}
        }
    }

    Ok(Header { titles, references })
}

fn parse_body(element: &Element, doc: &Document) -> Result<Body> {
    let mut descriptions = HashMap::new();
    let mut examples = None;

    for child in element.children(doc) {
        match child.name(doc) {
            "description" => {
                let lang = child
                    .attribute(doc, "xml:lang")
                    .or_else(|| child.attribute(doc, "lang"))
                    .unwrap_or("en")
                    .to_string();
                let text = get_element_text(&child, doc);
                descriptions.insert(lang, text);
            }
            "examples" => {
                examples = Some(parse_examples(&child, doc)?);
            }
            _ => {}
        }
    }

    Ok(Body {
        descriptions,
        examples,
    })
}

fn parse_references(element: &Element, doc: &Document) -> Result<Vec<Reference>> {
    let mut refs = Vec::new();

    for child in element.children(doc) {
        if child.name(doc) == "ref" {
            let n = child.attribute(doc, "n").unwrap_or("").to_string();
            refs.push(Reference { n });
        }
    }

    Ok(refs)
}

fn parse_examples(element: &Element, doc: &Document) -> Result<Vec<Example>> {
    let mut examples = Vec::new();

    for child in element.children(doc) {
        if child.name(doc) == "ex" {
            let lang = child.attribute(doc, "lang").unwrap_or("en").to_string();
            let text = get_element_text(&child, doc);
            examples.push(Example { lang, text });
        }
    }

    Ok(examples)
}

fn get_element_text(element: &Element, doc: &Document) -> String {
    let mut text = String::new();
    for node in element.child_nodes(doc) {
        if let Node::Text(text_node) = node {
            text.push_str(text_node.as_str(doc));
        }
    }
    text.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_ascii() {
        assert_eq!(encode_unicode_identifier("hello-world"), "hello-world");
        assert_eq!(encode_unicode_identifier("Hello World"), "hello-world");
        assert_eq!(encode_unicode_identifier("test123"), "test123");
    }

    #[test]
    fn test_encode_punctuation() {
        assert_eq!(encode_unicode_identifier("hello.world"), "hello_world");
        assert_eq!(encode_unicode_identifier("test@example"), "test_example");
    }

    #[test]
    fn test_encode_unicode() {
        assert_eq!(encode_unicode_identifier("café"), "caf_u00E9");
        assert_eq!(encode_unicode_identifier("föö"), "f_u00F6_u00F6");
        assert_eq!(
            encode_unicode_identifier("Test föö bår"),
            "test-f_u00F6_u00F6-b_u00E5r"
        );
    }

    #[test]
    fn test_encode_emoji() {
        // Emoji 🎉 is U+1F389, which is > 0xFFFF, so uses _U format
        assert_eq!(encode_unicode_identifier("test🎉"), "test_U01F389");
    }

    #[test]
    fn test_decode_ascii() {
        assert_eq!(decode_unicode_identifier("hello-world"), "hello-world");
        assert_eq!(decode_unicode_identifier("test_example"), "test_example");
    }

    #[test]
    fn test_decode_unicode() {
        assert_eq!(decode_unicode_identifier("caf_u00E9"), "café");
        assert_eq!(decode_unicode_identifier("f_u00F6_u00F6"), "föö");
        assert_eq!(
            decode_unicode_identifier("test-f_u00F6_u00F6-b_u00E5r"),
            "test-föö-bår"
        );
    }

    #[test]
    fn test_decode_emoji() {
        assert_eq!(decode_unicode_identifier("test_U01F389"), "test🎉");
    }

    #[test]
    fn test_roundtrip() {
        let test_cases = vec![
            "hello world",
            "café",
            "föö bår",
            "test🎉emoji",
            "Real-word error",
            "Rættstavingsfeilur",
        ];

        for test in test_cases {
            let encoded = encode_unicode_identifier(test);
            let decoded = decode_unicode_identifier(&encoded);

            // Note: encoding is lossy (spaces->hyphens, punctuation->underscores)
            // so we compare the encoded version
            let re_encoded = encode_unicode_identifier(&decoded);
            assert_eq!(encoded, re_encoded, "Roundtrip failed for: {}", test);
        }
    }
}
