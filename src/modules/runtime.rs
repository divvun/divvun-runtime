use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use divvun_runtime_macros::rt_command;

use crate::{ast, bundle::Bundle};

use super::{CommandRunner, Context, Input};

/// Forward input through a pipeline bundle
#[derive(facet::Facet)]
pub struct Forward {
    #[facet(opaque)]
    bundle: Bundle,
}

#[rt_command(
    module = "runtime",
    name = "forward",
    input = [String],
    output = "String",
    args = [pipeline_path = "Path"]
)]
impl Forward {
    pub fn new(
        _context: Arc<Context>,
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
        _input: Input,
        _config: Arc<serde_json::Value>,
    ) -> Result<Input, crate::modules::Error> {
        // let output = self
        //     .bundle
        //     .run_pipeline(input, config.to_owned())
        //     .await
        //     .map_err(|e| crate::modules::Error(e.to_string()))?;
        // Ok(output)
        todo!()
    }

    fn name(&self) -> &'static str {
        "runtime::forward"
    }
}
