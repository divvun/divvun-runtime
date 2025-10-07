use crate::state::PlaygroundState;
use crate::syntax;
use divvun_runtime::{
    ast::Command,
    bundle::Bundle,
    modules::{Input, InputEvent},
    util::fluent_loader::FluentLoader,
};
use fluent_bundle::FluentArgs;
use fluent_syntax::ast::{Expression, InlineExpression, PatternElement};
use futures_util::{FutureExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::io::Read as _;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundleInfo {
    pub id: String,
    pub path: String,
    pub name: String,
    pub commands: HashMap<String, CommandInfo>,
    pub entry: EntryInfo,
    pub output: RefInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntryInfo {
    pub value_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefInfo {
    pub r#ref: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandInfo {
    pub module: String,
    pub command: String,
    pub returns: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PipelineStepPayload {
    pub execution_id: String,
    pub step_index: usize,
    pub command_key: String,
    pub command: serde_json::Value,
    pub command_display: String,
    pub event_html: String,
    pub kind: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FluentFileInfo {
    pub path: String,
    pub locale: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FluentMessageInfo {
    pub id: String,
    pub has_desc: bool,
    pub detected_params: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FluentMessageResult {
    pub title: String,
    pub description: String,
}

fn create_bundle_info(id: String, path: String, bundle: &Bundle) -> BundleInfo {
    let defn = bundle.definition();
    let name = PathBuf::from(&path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("Unknown")
        .to_string();

    let commands = defn
        .commands
        .iter()
        .map(|(k, v)| {
            (
                k.clone(),
                CommandInfo {
                    module: v.module.clone(),
                    command: v.command.clone(),
                    returns: v.returns.clone(),
                },
            )
        })
        .collect();

    BundleInfo {
        id,
        path,
        name,
        commands,
        entry: EntryInfo {
            value_type: defn.entry.value_type.clone(),
        },
        output: RefInfo {
            r#ref: defn.output.r#ref.clone(),
        },
    }
}

#[tauri::command]
pub async fn load_bundle(
    path: String,
    state: State<'_, PlaygroundState>,
) -> Result<BundleInfo, String> {
    tracing::info!("Loading bundle from: {}", path);

    let bundle = if path.ends_with(".drb") {
        Bundle::from_bundle(&path).map_err(|e| format!("Failed to load bundle: {}", e))?
    } else {
        Bundle::from_path(&path).map_err(|e| format!("Failed to load bundle: {}", e))?
    };

    let bundle_id = uuid::Uuid::new_v4().to_string();
    let bundle_info = create_bundle_info(bundle_id.clone(), path, &bundle);

    let mut bundles = state.bundles.lock().await;
    bundles.insert(bundle_id, bundle);

    Ok(bundle_info)
}

fn determine_kind(cmd: &Command, event: &InputEvent) -> Option<String> {
    // Fall back to content-based detection for JSON
    if let InputEvent::Input(Input::Json(_)) = event {
        return Some("json".to_string());
    }

    // First, check if command explicitly specifies a kind (from CommandDef or ast.json)
    if let Some(ref kind) = cmd.kind {
        return Some(kind.clone());
    }

    // Default to plain text
    None
}

#[tauri::command]
pub async fn run_pipeline(
    bundle_id: String,
    input: String,
    app_handle: AppHandle,
    state: State<'_, PlaygroundState>,
) -> Result<String, String> {
    tracing::info!("Running pipeline for bundle: {}", bundle_id);

    let bundles = state.bundles.lock().await;
    let bundle = bundles
        .get(&bundle_id)
        .ok_or_else(|| "Bundle not found".to_string())?;

    let execution_id = uuid::Uuid::new_v4().to_string();
    let execution_id_clone = execution_id.clone();
    let app_handle_clone = app_handle.clone();

    // Create tap function to emit events
    let tap = Arc::new(move |key: &str, cmd: &Command, event: &InputEvent| {
        let execution_id = execution_id_clone.clone();
        let app_handle = app_handle_clone.clone();
        let command_key = key.to_string();
        let command_json = serde_json::to_value(cmd).unwrap_or_default();
        let kind = determine_kind(cmd, event);

        // Format the event output - pretty-print JSON based on data type
        let event_str = match event {
            InputEvent::Input(Input::Json(ref val)) => {
                // Always pretty-print JSON data, regardless of kind
                serde_json::to_string_pretty(val).unwrap_or_else(|_| format!("{:#}", event))
            }
            _ => format!("{:#}", event),
        };

        // Convert to HTML with syntax highlighting
        let event_html = if let Some(ref syntax_kind) = kind {
            syntax::highlight_to_html(&event_str, syntax_kind)
        } else {
            html_escape::encode_text(&event_str).to_string()
        };

        let command_display = cmd.as_str(false);

        async move {
            // Emit event to frontend
            let payload = PipelineStepPayload {
                execution_id: execution_id.clone(),
                step_index: 0, // We'll increment this in the frontend
                command_key,
                command: command_json,
                command_display,
                event_html,
                kind,
            };

            if let Err(e) = app_handle.emit("pipeline-step", payload) {
                tracing::error!("Failed to emit pipeline-step event: {}", e);
            }

            divvun_runtime::modules::TapOutput::Continue
        }
        .boxed()
    });

    // Create pipeline with tap
    let mut pipe = bundle
        .create_with_tap(serde_json::json!({}), tap)
        .await
        .map_err(|e| format!("Failed to create pipeline: {}", e))?;

    // Run pipeline
    let mut stream = pipe.forward(Input::String(input)).await;
    let mut final_output = String::new();

    while let Some(result) = stream.next().await {
        match result {
            Ok(output) => {
                final_output = format!("{:#}", output);
            }
            Err(e) => {
                return Err(format!("Pipeline error: {}", e));
            }
        }
    }

    Ok(final_output)
}

#[tauri::command]
pub async fn list_ftl_files(
    bundle_id: String,
    state: State<'_, PlaygroundState>,
) -> Result<Vec<FluentFileInfo>, String> {
    tracing::info!("Listing .ftl files for bundle: {}", bundle_id);

    let bundles = state.bundles.lock().await;
    let bundle = bundles
        .get(&bundle_id)
        .ok_or_else(|| "Bundle not found".to_string())?;

    let context = bundle.context();
    let files = context
        .load_files_glob("*.ftl")
        .map_err(|e| format!("Failed to load .ftl files: {}", e))?;

    let mut ftl_files = Vec::new();
    for (path, _) in files {
        let filename = path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| "Invalid filename".to_string())?;

        // Extract locale from filename like "errors-en.ftl" -> "en"
        let locale = if let Some(stem) = filename.strip_suffix(".ftl") {
            if let Some(dash_pos) = stem.rfind('-') {
                stem[dash_pos + 1..].to_string()
            } else {
                "unknown".to_string()
            }
        } else {
            "unknown".to_string()
        };

        ftl_files.push(FluentFileInfo {
            path: filename.to_string(),
            locale,
        });
    }

    Ok(ftl_files)
}

/// Recursively extract all variable references from a Fluent pattern
fn extract_variables_from_pattern<S>(pattern: &fluent_syntax::ast::Pattern<S>) -> HashSet<String>
where
    S: AsRef<str>,
{
    let mut variables = HashSet::new();

    for element in &pattern.elements {
        if let PatternElement::Placeable { expression } = element {
            extract_variables_from_expression(expression, &mut variables);
        }
    }

    variables
}

/// Recursively extract variable references from an expression
fn extract_variables_from_expression<S>(
    expression: &Expression<S>,
    variables: &mut HashSet<String>,
) where
    S: AsRef<str>,
{
    match expression {
        Expression::Inline(inline) => {
            extract_variables_from_inline(inline, variables);
        }
        Expression::Select { selector, variants } => {
            extract_variables_from_inline(selector, variables);
            for variant in variants {
                for element in &variant.value.elements {
                    if let PatternElement::Placeable { expression } = element {
                        extract_variables_from_expression(expression, variables);
                    }
                }
            }
        }
    }
}

/// Extract variable references from an inline expression
fn extract_variables_from_inline<S>(
    inline: &InlineExpression<S>,
    variables: &mut HashSet<String>,
) where
    S: AsRef<str>,
{
    match inline {
        InlineExpression::VariableReference { id } => {
            variables.insert(id.name.as_ref().to_string());
        }
        InlineExpression::FunctionReference { arguments, .. } => {
            // Check positional arguments
            for arg in &arguments.positional {
                extract_variables_from_inline(arg, variables);
            }
            // Check named arguments
            for arg in &arguments.named {
                extract_variables_from_inline(&arg.value, variables);
            }
        }
        InlineExpression::MessageReference { attribute, .. } => {
            // Message references don't contain variables themselves
            if let Some(_attr) = attribute {
                // Attributes don't contain variables in their identifiers
            }
        }
        InlineExpression::TermReference { attribute, arguments, .. } => {
            if let Some(_attr) = attribute {
                // Attributes don't contain variables
            }
            if let Some(args) = arguments {
                for arg in &args.positional {
                    extract_variables_from_inline(arg, variables);
                }
                for arg in &args.named {
                    extract_variables_from_inline(&arg.value, variables);
                }
            }
        }
        InlineExpression::Placeable { expression } => {
            extract_variables_from_expression(expression, variables);
        }
        InlineExpression::StringLiteral { .. }
        | InlineExpression::NumberLiteral { .. } => {
            // Literals don't contain variables
        }
    }
}

#[tauri::command]
pub async fn get_ftl_messages(
    bundle_id: String,
    file_path: String,
    state: State<'_, PlaygroundState>,
) -> Result<Vec<FluentMessageInfo>, String> {
    tracing::info!(
        "Getting messages from .ftl file: {} in bundle: {}",
        file_path,
        bundle_id
    );

    let bundles = state.bundles.lock().await;
    let bundle = bundles
        .get(&bundle_id)
        .ok_or_else(|| "Bundle not found".to_string())?;

    let context = bundle.context();
    let mut reader = context
        .load_file(&file_path)
        .map_err(|e| format!("Failed to load file {}: {}", file_path, e))?;

    let mut content = String::new();
    reader
        .read_to_string(&mut content)
        .map_err(|e| format!("Failed to read file {}: {}", file_path, e))?;

    // Parse the Fluent resource
    let resource = fluent::FluentResource::try_new(content)
        .map_err(|e| format!("Failed to parse Fluent file: {:?}", e.1))?;

    let mut messages = Vec::new();
    for entry in resource.entries() {
        if let fluent_syntax::ast::Entry::Message(msg) = entry {
            let id = msg.id.name.to_string();
            let has_desc = msg.attributes.iter().any(|attr| attr.id.name == "desc");

            // Properly extract variable references from the AST
            let mut detected_params = HashSet::new();

            // Extract from message value
            if let Some(ref pattern) = msg.value {
                detected_params.extend(extract_variables_from_pattern(pattern));
            }

            // Extract from description attribute
            if let Some(desc_attr) = msg.attributes.iter().find(|attr| attr.id.name == "desc") {
                detected_params.extend(extract_variables_from_pattern(&desc_attr.value));
            }

            let mut params: Vec<String> = detected_params.into_iter().collect();
            params.sort();

            messages.push(FluentMessageInfo {
                id,
                has_desc,
                detected_params: params,
            });
        }
    }

    Ok(messages)
}

#[tauri::command]
pub async fn test_ftl_message(
    bundle_id: String,
    locale: String,
    message_id: String,
    args: HashMap<String, String>,
    state: State<'_, PlaygroundState>,
) -> Result<FluentMessageResult, String> {
    tracing::info!(
        "Testing message {} with locale {} in bundle {}",
        message_id,
        locale,
        bundle_id
    );

    let bundles = state.bundles.lock().await;
    let bundle = bundles
        .get(&bundle_id)
        .ok_or_else(|| "Bundle not found".to_string())?;

    let context = bundle.context();

    // Load the FluentLoader
    let fluent_loader = FluentLoader::new(context.clone(), "*.ftl", &locale)
        .map_err(|e| format!("Failed to create FluentLoader: {}", e))?;

    // Convert args to FluentArgs
    let mut fluent_args = FluentArgs::new();
    for (key, value) in &args {
        tracing::debug!("Setting fluent arg: {} = {}", key, value);
        fluent_args.set(key, value);
    }

    tracing::debug!("Calling get_message with locale={}, message_id={}, args={:?}", locale, message_id, args);

    // Get the message
    let (title, description) = fluent_loader
        .get_message(Some(&locale), &message_id, Some(&fluent_args))
        .map_err(|e| format!("Failed to get message: {}", e))?;

    tracing::debug!("Got result: title={}, desc={}", title, description);

    Ok(FluentMessageResult {
        title,
        description,
    })
}
