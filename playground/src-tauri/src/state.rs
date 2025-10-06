use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use divvun_runtime::bundle::Bundle;

pub struct PlaygroundState {
    pub bundles: Arc<Mutex<HashMap<String, Bundle>>>,
}

impl PlaygroundState {
    pub fn new() -> Self {
        Self {
            bundles: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}
