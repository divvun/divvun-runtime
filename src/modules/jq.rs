use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use divvun_runtime_macros::rt_command;

use crate::ast;

use super::{CommandRunner, Error, Input};

/// JSON query processor using jq syntax
#[derive(facet::Facet)]
pub struct Jq {
    filter: String,
}

#[rt_command(
    module = "jq",
    name = "jq",
    input = [Json],
    output = "Json",
    args = [filter = "String"]
)]
impl Jq {
    pub fn new(
        _context: Arc<super::Context>,
        mut kwargs: HashMap<String, ast::Arg>,
    ) -> Result<Arc<dyn CommandRunner + Send + Sync>, super::Error> {
        let filter = kwargs
            .remove("filter")
            .and_then(|x| x.value)
            .and_then(|x| x.try_as_string())
            .ok_or_else(|| Error("filter missing".to_string()))?;

        Ok(Arc::new(Self { filter }) as _)
    }
}

#[async_trait]
impl CommandRunner for Jq {
    async fn forward(
        self: Arc<Self>,
        input: Input,
        _config: Arc<serde_json::Value>,
    ) -> Result<Input, crate::modules::Error> {
        use jaq_core::load::{Arena, File, Loader};
        use jaq_json::Val;

        let json_input = input.try_into_json()?;

        // Set up jaq components
        let arena = Arena::default();
        let loader = Loader::new(jaq_std::defs().chain(jaq_json::defs()));
        let program = File {
            code: self.filter.as_str(),
            path: (),
        };

        // Parse the filter
        let modules = loader
            .load(&arena, program)
            .map_err(|e| Error(format!("Failed to parse jq filter: {:?}", e)))?;

        // Compile the filter
        let filter = jaq_core::Compiler::default()
            .with_funs(jaq_std::funs().chain(jaq_json::funs()))
            .compile(modules)
            .map_err(|e| Error(format!("Failed to compile jq filter: {:?}", e)))?;

        // Create execution context
        let inputs = jaq_core::RcIter::new(core::iter::empty());
        let ctx = jaq_core::Ctx::new([], &inputs);

        // Convert input to jaq Val type
        let input_val = Val::from(json_input);

        // Execute the filter and collect results
        let results: Result<Vec<serde_json::Value>, _> = filter
            .run((ctx, input_val))
            .map(|result| match result {
                Ok(val) => Ok(serde_json::Value::from(val)),
                Err(e) => Err(Error(format!("Filter execution error: {:?}", e))),
            })
            .collect();

        let results = results?;

        // Return results based on count
        match results.len() {
            0 => Ok(Input::Json(serde_json::Value::Null)),
            1 => Ok(Input::Json(results.into_iter().next().unwrap())),
            _ => Ok(Input::Json(serde_json::Value::Array(results))),
        }
    }

    fn name(&self) -> &'static str {
        "jq::jq"
    }
}
