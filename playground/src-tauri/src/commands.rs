use crate::state::PlaygroundState;
use crate::syntax;
use divvun_runtime::{
    ast::Command,
    bundle::Bundle,
    modules::{Input, InputEvent},
};
use futures_util::{FutureExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
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
    pub event_html: String,
    pub kind: Option<String>,
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

        async move {
            // Emit event to frontend
            let payload = PipelineStepPayload {
                execution_id: execution_id.clone(),
                step_index: 0, // We'll increment this in the frontend
                command_key,
                command: command_json,
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
