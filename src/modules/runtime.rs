use std::{collections::HashMap, sync::Arc, thread::JoinHandle};

use async_trait::async_trait;
use box_format::BoxPath;
use tokio::sync::{
    mpsc::{self, Receiver, Sender},
    Mutex,
};

use crate::{
    ast,
    modules::{Arg, Command, Module, Ty},
    Bundle,
};

use super::{CommandRunner, Context, Input, SharedInputFut};

inventory::submit! {
    Module {
        name: "runtime",
        commands: &[
            Command {
                name: "forward",
                input: &[Ty::String],
                args: &[Arg { name: "pipeline_path", ty: Ty::Path }],
                init: Forward::new,
                returns: Ty::String,
            }
        ]
    }
}

pub struct Forward {
    bundle: Bundle,
}

impl Forward {
    pub fn new(
        context: Arc<Context>,
        mut kwargs: HashMap<String, ast::Arg>,
    ) -> Result<Arc<dyn CommandRunner + Send + Sync>, crate::modules::Error> {
        tracing::debug!("Creating forward");
        let bundle_path = kwargs
            .remove("pipeline_path")
            .and_then(|x| x.value)
            .and_then(|x| x.try_as_string())
            .ok_or_else(|| crate::modules::Error("pipeline_path missing".to_string()))?;

        let bundle =
            Bundle::from_bundle(bundle_path).map_err(|e| crate::modules::Error(e.to_string()))?;

        Ok(Arc::new(Self { bundle }) as _)
    }
}

#[async_trait]
impl CommandRunner for Forward {
    async fn forward(
        self: Arc<Self>,
        input: SharedInputFut,
        config: Arc<serde_json::Value>,
    ) -> Result<Input, crate::modules::Error> {
        let input = input.await?;
        let output = self
            .bundle
            .run_pipeline(input, config.to_owned())
            .await
            .map_err(|e| crate::modules::Error(e.to_string()))?;
        Ok(output)
    }

    fn name(&self) -> &'static str {
        "runtime::forward"
    }
}
