use anyhow::{anyhow, Result};
use heck::ToKebabCase;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use xmlem::{Document, Element, Node};

pub mod fluent;

#[derive(Debug, Serialize, Deserialize)]
pub struct ErrorDocument {
    pub defaults: Vec<Default>,
    pub errors: Vec<Error>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Default {
    pub id: String,
    pub ids: Vec<Id>,
    pub header: Header,
    pub body: Body,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Error {
    pub id: String,
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

    // Generate kebab-case ID from first English title
    let id = header
        .titles
        .get("en")
        .map(|title| title.to_kebab_case())
        .unwrap_or_else(|| "unknown-default".to_string());

    Ok(Default {
        id,
        ids,
        header,
        body,
    })
}

fn parse_error(element: &Element, doc: &Document) -> Result<Error> {
    let id = element
        .attribute(doc, "id")
        .ok_or_else(|| anyhow!("Error missing id attribute"))?
        .to_string();

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
                let lang = child.attribute(doc, "lang").unwrap_or("en").to_string();
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
                let lang = child.attribute(doc, "lang").unwrap_or("en").to_string();
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
