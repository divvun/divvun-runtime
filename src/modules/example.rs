use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use divvun_runtime_macros::rt_command;

use crate::ast;

use super::{CommandRunner, Context, Input, SharedInputFut};

/// Reverses the input string
#[derive(facet::Facet)]
pub struct Reverse;

#[rt_command(
    module = "example",
    name = "reverse", 
    input = [String],
    output = "String",
    args = []
)]
impl Reverse {
    pub fn new(
        _context: Arc<Context>,
        _kwargs: HashMap<String, ast::Arg>,
    ) -> Result<Arc<dyn CommandRunner + Send + Sync>, super::Error> {
        Ok(Arc::new(Self) as _)
    }
}

#[async_trait]
impl CommandRunner for Reverse {
    async fn forward(
        self: Arc<Self>,
        input: Input,
        _config: Arc<serde_json::Value>,
    ) -> Result<Input, crate::modules::Error> {
        let input = input.try_into_string()?;
        Ok(input.chars().rev().collect::<String>().into())
    }

    fn name(&self) -> &'static str {
        "example::reverse"
    }
}

/// Converts input string to uppercase
#[derive(facet::Facet)]
pub struct Upper;

#[rt_command(
    module = "example",
    name = "upper",
    input = [String],
    output = "String",
    args = []
)]
impl Upper {
    pub fn new(
        _context: Arc<Context>,
        _kwargs: HashMap<String, ast::Arg>,
    ) -> Result<Arc<dyn CommandRunner + Send + Sync>, super::Error> {
        Ok(Arc::new(Self) as _)
    }
}

#[async_trait]
impl CommandRunner for Upper {
    async fn forward(
        self: Arc<Self>,
        input: Input,
        _config: Arc<serde_json::Value>,
    ) -> Result<Input, crate::modules::Error> {
        let input = input.try_into_string()?;
        Ok(input.to_uppercase().into())
    }

    fn name(&self) -> &'static str {
        "example::upper"
    }
}
