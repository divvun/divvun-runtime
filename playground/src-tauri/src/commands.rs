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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TabInfo {
    pub tab_id: String,
    pub bundle_name: Option<String>,
    pub current_view: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowStateInfo {
    pub window_id: String,
    pub tabs: Vec<TabInfo>,
    pub active_tab_index: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TabData {
    pub tab_id: String,
    pub bundle_info: Option<BundleInfo>,
    pub current_view: String,
    pub pipeline_input: String,
    pub fluent_file: Option<String>,
    pub fluent_message: Option<String>,
    pub fluent_args: HashMap<String, String>,
}

#[tauri::command]
pub async fn init_window(
    window_id: String,
    state: State<'_, PlaygroundState>,
) -> Result<WindowStateInfo, String> {
    tracing::info!("Initializing window: {}", window_id);

    let mut windows = state.windows.lock().await;

    let window_state = windows
        .entry(window_id.clone())
        .or_insert_with(|| crate::state::WindowState::new(window_id.clone()));

    let tabs_info: Vec<TabInfo> = window_state
        .tabs
        .iter()
        .map(|t| TabInfo {
            tab_id: t.tab_id.clone(),
            bundle_name: t.bundle_info.as_ref().map(|b| b.name.clone()),
            current_view: t.current_view.clone(),
        })
        .collect();

    Ok(WindowStateInfo {
        window_id: window_state.window_id.clone(),
        tabs: tabs_info,
        active_tab_index: window_state.active_tab_index,
    })
}

#[tauri::command]
pub async fn get_window_state(
    window_id: String,
    state: State<'_, PlaygroundState>,
) -> Result<WindowStateInfo, String> {
    tracing::info!("Getting window state: {}", window_id);

    let windows = state.windows.lock().await;
    let window_state = windows
        .get(&window_id)
        .ok_or_else(|| "Window not found".to_string())?;

    let tabs_info: Vec<TabInfo> = window_state
        .tabs
        .iter()
        .map(|t| TabInfo {
            tab_id: t.tab_id.clone(),
            bundle_name: t.bundle_info.as_ref().map(|b| b.name.clone()),
            current_view: t.current_view.clone(),
        })
        .collect();

    Ok(WindowStateInfo {
        window_id: window_state.window_id.clone(),
        tabs: tabs_info,
        active_tab_index: window_state.active_tab_index,
    })
}

#[tauri::command]
pub async fn create_tab(
    window_id: String,
    state: State<'_, PlaygroundState>,
) -> Result<TabInfo, String> {
    tracing::info!("Creating tab in window: {}", window_id);

    let mut windows = state.windows.lock().await;
    let window_state = windows
        .get_mut(&window_id)
        .ok_or_else(|| "Window not found".to_string())?;

    let new_tab = crate::state::TabState::new(uuid::Uuid::new_v4().to_string());
    let tab_info = TabInfo {
        tab_id: new_tab.tab_id.clone(),
        bundle_name: None,
        current_view: new_tab.current_view.clone(),
    };

    window_state.tabs.push(new_tab);
    window_state.active_tab_index = window_state.tabs.len() - 1;

    Ok(tab_info)
}

#[tauri::command]
pub async fn close_tab(
    window_id: String,
    tab_id: String,
    state: State<'_, PlaygroundState>,
) -> Result<(), String> {
    tracing::info!("Closing tab {} in window {}", tab_id, window_id);

    let mut windows = state.windows.lock().await;
    let window_state = windows
        .get_mut(&window_id)
        .ok_or_else(|| "Window not found".to_string())?;

    if window_state.tabs.len() <= 1 {
        return Err("Cannot close the last tab".to_string());
    }

    let tab_index = window_state
        .tabs
        .iter()
        .position(|t| t.tab_id == tab_id)
        .ok_or_else(|| "Tab not found".to_string())?;

    window_state.tabs.remove(tab_index);

    if window_state.active_tab_index >= window_state.tabs.len() {
        window_state.active_tab_index = window_state.tabs.len() - 1;
    }

    Ok(())
}

#[tauri::command]
pub async fn switch_tab(
    window_id: String,
    tab_index: usize,
    state: State<'_, PlaygroundState>,
) -> Result<(), String> {
    tracing::info!("Switching to tab {} in window {}", tab_index, window_id);

    let mut windows = state.windows.lock().await;
    let window_state = windows
        .get_mut(&window_id)
        .ok_or_else(|| "Window not found".to_string())?;

    if tab_index >= window_state.tabs.len() {
        return Err("Invalid tab index".to_string());
    }

    window_state.active_tab_index = tab_index;

    Ok(())
}

#[tauri::command]
pub async fn duplicate_tab(
    window_id: String,
    tab_id: String,
    state: State<'_, PlaygroundState>,
) -> Result<TabInfo, String> {
    tracing::info!("Duplicating tab {} in window {}", tab_id, window_id);

    let mut windows = state.windows.lock().await;
    let window_state = windows
        .get_mut(&window_id)
        .ok_or_else(|| "Window not found".to_string())?;

    let source_tab = window_state
        .get_tab_by_id(&tab_id)
        .ok_or_else(|| "Tab not found".to_string())?;

    let mut new_tab = source_tab.clone();
    new_tab.tab_id = uuid::Uuid::new_v4().to_string();
    new_tab.pipeline_steps.clear();

    let tab_info = TabInfo {
        tab_id: new_tab.tab_id.clone(),
        bundle_name: new_tab.bundle_info.as_ref().map(|b| b.name.clone()),
        current_view: new_tab.current_view.clone(),
    };

    window_state.tabs.push(new_tab);
    window_state.active_tab_index = window_state.tabs.len() - 1;

    Ok(tab_info)
}

#[tauri::command]
pub async fn get_tab_data(
    window_id: String,
    tab_id: String,
    state: State<'_, PlaygroundState>,
) -> Result<TabData, String> {
    tracing::info!(
        "Getting tab data for tab {} in window {}",
        tab_id,
        window_id
    );

    let windows = state.windows.lock().await;
    let window_state = windows
        .get(&window_id)
        .ok_or_else(|| "Window not found".to_string())?;

    let tab = window_state
        .get_tab_by_id(&tab_id)
        .ok_or_else(|| "Tab not found".to_string())?;

    Ok(TabData {
        tab_id: tab.tab_id.clone(),
        bundle_info: tab.bundle_info.clone(),
        current_view: tab.current_view.clone(),
        pipeline_input: tab.pipeline_input.clone(),
        fluent_file: tab.fluent_file.clone(),
        fluent_message: tab.fluent_message.clone(),
        fluent_args: tab.fluent_args.clone(),
    })
}

#[tauri::command]
pub async fn update_tab_input(
    window_id: String,
    tab_id: String,
    input: String,
    state: State<'_, PlaygroundState>,
) -> Result<(), String> {
    tracing::debug!("Updating input for tab {} in window {}", tab_id, window_id);

    let mut windows = state.windows.lock().await;
    let window_state = windows
        .get_mut(&window_id)
        .ok_or_else(|| "Window not found".to_string())?;

    let tab = window_state
        .get_tab_by_id_mut(&tab_id)
        .ok_or_else(|| "Tab not found".to_string())?;

    tab.pipeline_input = input;

    Ok(())
}

#[tauri::command]
pub async fn update_tab_view(
    window_id: String,
    tab_id: String,
    view: String,
    state: State<'_, PlaygroundState>,
) -> Result<(), String> {
    tracing::debug!(
        "Updating view for tab {} in window {} to {}",
        tab_id,
        window_id,
        view
    );

    let mut windows = state.windows.lock().await;
    let window_state = windows
        .get_mut(&window_id)
        .ok_or_else(|| "Window not found".to_string())?;

    let tab = window_state
        .get_tab_by_id_mut(&tab_id)
        .ok_or_else(|| "Tab not found".to_string())?;

    tab.current_view = view;

    Ok(())
}

#[tauri::command]
pub async fn load_bundle(
    window_id: String,
    tab_id: String,
    path: String,
    state: State<'_, PlaygroundState>,
) -> Result<BundleInfo, String> {
    tracing::info!(
        "Loading bundle from: {} for tab {} in window {}",
        path,
        tab_id,
        window_id
    );

    let bundle = if path.ends_with(".drb") {
        Bundle::from_bundle(&path).map_err(|e| format!("Failed to load bundle: {}", e))?
    } else {
        Bundle::from_path(&path).map_err(|e| format!("Failed to load bundle: {}", e))?
    };

    let bundle_id = uuid::Uuid::new_v4().to_string();
    let bundle_info = create_bundle_info(bundle_id.clone(), path, &bundle);

    let mut windows = state.windows.lock().await;
    let window_state = windows
        .get_mut(&window_id)
        .ok_or_else(|| "Window not found".to_string())?;

    let tab = window_state
        .get_tab_by_id_mut(&tab_id)
        .ok_or_else(|| "Tab not found".to_string())?;

    tab.bundle = Some(Arc::new(bundle));
    tab.bundle_info = Some(bundle_info.clone());
    tab.pipeline_steps.clear();

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
    window_id: String,
    tab_id: String,
    input: String,
    app_handle: AppHandle,
    state: State<'_, PlaygroundState>,
) -> Result<String, String> {
    tracing::info!(
        "Running pipeline for tab {} in window {}",
        tab_id,
        window_id
    );

    let windows = state.windows.lock().await;
    let window_state = windows
        .get(&window_id)
        .ok_or_else(|| "Window not found".to_string())?;

    let tab = window_state
        .get_tab_by_id(&tab_id)
        .ok_or_else(|| "Tab not found".to_string())?;

    let bundle = tab
        .bundle
        .as_ref()
        .ok_or_else(|| "No bundle loaded in tab".to_string())?;

    let execution_id = uuid::Uuid::new_v4().to_string();
    let execution_id_clone = execution_id.clone();
    let app_handle_clone = app_handle.clone();
    let window_id_clone = window_id.clone();
    let tab_id_clone = tab_id.clone();

    // Create tap function to emit events
    let tap = Arc::new(move |key: &str, cmd: &Command, event: &InputEvent| {
        let execution_id = execution_id_clone.clone();
        let app_handle = app_handle_clone.clone();
        let window_id = window_id_clone.clone();
        let tab_id = tab_id_clone.clone();
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
            // Emit event to frontend with window and tab context
            #[derive(Serialize, Clone)]
            struct PipelineStepEvent {
                window_id: String,
                tab_id: String,
                execution_id: String,
                step_index: usize,
                command_key: String,
                command: serde_json::Value,
                command_display: String,
                event_html: String,
                kind: Option<String>,
            }

            let payload = PipelineStepEvent {
                window_id,
                tab_id,
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
    window_id: String,
    tab_id: String,
    state: State<'_, PlaygroundState>,
) -> Result<Vec<FluentFileInfo>, String> {
    tracing::info!(
        "Listing .ftl files for tab {} in window {}",
        tab_id,
        window_id
    );

    let windows = state.windows.lock().await;
    let window_state = windows
        .get(&window_id)
        .ok_or_else(|| "Window not found".to_string())?;

    let tab = window_state
        .get_tab_by_id(&tab_id)
        .ok_or_else(|| "Tab not found".to_string())?;

    let bundle = tab
        .bundle
        .as_ref()
        .ok_or_else(|| "No bundle loaded in tab".to_string())?;

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
fn extract_variables_from_expression<S>(expression: &Expression<S>, variables: &mut HashSet<String>)
where
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
fn extract_variables_from_inline<S>(inline: &InlineExpression<S>, variables: &mut HashSet<String>)
where
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
        InlineExpression::TermReference {
            attribute,
            arguments,
            ..
        } => {
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
        InlineExpression::StringLiteral { .. } | InlineExpression::NumberLiteral { .. } => {
            // Literals don't contain variables
        }
    }
}

#[tauri::command]
pub async fn get_ftl_messages(
    window_id: String,
    tab_id: String,
    file_path: String,
    state: State<'_, PlaygroundState>,
) -> Result<Vec<FluentMessageInfo>, String> {
    tracing::info!(
        "Getting messages from .ftl file: {} for tab {} in window {}",
        file_path,
        tab_id,
        window_id
    );

    let windows = state.windows.lock().await;
    let window_state = windows
        .get(&window_id)
        .ok_or_else(|| "Window not found".to_string())?;

    let tab = window_state
        .get_tab_by_id(&tab_id)
        .ok_or_else(|| "Tab not found".to_string())?;

    let bundle = tab
        .bundle
        .as_ref()
        .ok_or_else(|| "No bundle loaded in tab".to_string())?;

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
    window_id: String,
    tab_id: String,
    locale: String,
    message_id: String,
    args: HashMap<String, String>,
    state: State<'_, PlaygroundState>,
) -> Result<FluentMessageResult, String> {
    tracing::info!(
        "Testing message {} with locale {} for tab {} in window {}",
        message_id,
        locale,
        tab_id,
        window_id
    );

    let windows = state.windows.lock().await;
    let window_state = windows
        .get(&window_id)
        .ok_or_else(|| "Window not found".to_string())?;

    let tab = window_state
        .get_tab_by_id(&tab_id)
        .ok_or_else(|| "Tab not found".to_string())?;

    let bundle = tab
        .bundle
        .as_ref()
        .ok_or_else(|| "No bundle loaded in tab".to_string())?;

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

    tracing::debug!(
        "Calling get_message with locale={}, message_id={}, args={:?}",
        locale,
        message_id,
        args
    );

    // Get the message
    let (title, description) = fluent_loader
        .get_message(Some(&locale), &message_id, Some(&fluent_args))
        .map_err(|e| format!("Failed to get message: {}", e))?;

    tracing::debug!("Got result: title={}, desc={}", title, description);

    Ok(FluentMessageResult { title, description })
}
