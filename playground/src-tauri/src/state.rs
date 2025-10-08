use divvun_runtime::bundle::Bundle;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Clone, Serialize, Deserialize)]
pub struct TabState {
    pub tab_id: String,
    #[serde(skip)]
    pub bundle: Option<Arc<Bundle>>,
    pub bundle_info: Option<crate::commands::BundleInfo>,
    pub bundle_path: Option<String>,
    pub selected_pipeline: Option<String>,
    pub current_view: String, // "pipeline" or "fluent"
    pub pipeline_input: String,
    #[serde(skip)]
    pub pipeline_steps: Vec<crate::commands::PipelineStepPayload>,
    pub fluent_file: Option<String>,
    pub fluent_message: Option<String>,
    pub fluent_args: HashMap<String, String>,
}

impl TabState {
    pub fn new(tab_id: String) -> Self {
        Self {
            tab_id,
            bundle: None,
            bundle_info: None,
            bundle_path: None,
            selected_pipeline: None,
            current_view: "pipeline".to_string(),
            pipeline_input: String::new(),
            pipeline_steps: Vec::new(),
            fluent_file: None,
            fluent_message: None,
            fluent_args: HashMap::new(),
        }
    }
}

pub struct WindowState {
    pub window_id: String,
    pub tabs: Vec<TabState>,
    pub active_tab_index: usize,
}

impl WindowState {
    pub fn new(window_id: String) -> Self {
        let initial_tab = TabState::new(uuid::Uuid::new_v4().to_string());
        Self {
            window_id,
            tabs: vec![initial_tab],
            active_tab_index: 0,
        }
    }

    pub fn get_active_tab(&self) -> Option<&TabState> {
        self.tabs.get(self.active_tab_index)
    }

    pub fn get_active_tab_mut(&mut self) -> Option<&mut TabState> {
        self.tabs.get_mut(self.active_tab_index)
    }

    pub fn get_tab_by_id(&self, tab_id: &str) -> Option<&TabState> {
        self.tabs.iter().find(|t| t.tab_id == tab_id)
    }

    pub fn get_tab_by_id_mut(&mut self, tab_id: &str) -> Option<&mut TabState> {
        self.tabs.iter_mut().find(|t| t.tab_id == tab_id)
    }
}

pub struct PlaygroundState {
    pub windows: Arc<Mutex<HashMap<String, WindowState>>>,
}

impl PlaygroundState {
    pub fn new() -> Self {
        Self {
            windows: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}
