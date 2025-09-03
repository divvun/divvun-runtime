use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use divvun_runtime_macros::rt_command;

use crate::ast;

use super::{CommandRunner, Context, Input, SharedInputFut};

pub struct Strip;

#[rt_command(
    module = "ssml",
    name = "strip",
    input = [String],
    output = "String",
    args = []
)]
impl Strip {
    pub fn new(
        _context: Arc<Context>,
        _kwargs: HashMap<String, ast::Arg>,
    ) -> Result<Arc<dyn CommandRunner + Send + Sync>, super::Error> {
        Ok(Arc::new(Self) as _)
    }
}

#[async_trait]
impl CommandRunner for Strip {
    async fn forward(
        self: Arc<Self>,
        input: Input,
        _config: Arc<serde_json::Value>,
    ) -> Result<Input, crate::modules::Error> {
        let input = input.try_into_string()?;
        let output = tokio::task::spawn_blocking(move || {
            let ssml = ssml_parser::parse_ssml(&input)
                .map_err(|e| crate::modules::Error(e.to_string()))?;
            Ok::<_, crate::modules::Error>(ssml.get_text().to_string())
        })
        .await
        .unwrap()?;

        Ok(output.into())
    }

    fn name(&self) -> &'static str {
        "ssml::strip"
    }
}
