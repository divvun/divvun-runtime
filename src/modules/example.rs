use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;

use crate::{
    ast,
    modules::{Command, Module, Ty},
};

use super::{CommandRunner, Context, Input, SharedInputFut};

inventory::submit! {
    Module {
        name: "example",
        commands: &[
            Command {
                name: "reverse",
                input: &[Ty::String],
                args: &[],
                init: Reverse::new,
                returns: Ty::String,
            },
            Command {
                name: "upper",
                input: &[Ty::String],
                args: &[],
                init: Upper::new,
                returns: Ty::String,
            }
        ]
    }
}

pub struct Reverse;

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
        input: SharedInputFut,
    ) -> Result<Input, crate::modules::Error> {
        let input = input.await?.try_into_string()?;
        Ok(input.chars().rev().collect::<String>().into())
    }

    fn name(&self) -> &'static str {
        "example::reverse"
    }
}

pub struct Upper;

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
        input: SharedInputFut,
    ) -> Result<Input, crate::modules::Error> {
        let input = input.await?.try_into_string()?;
        Ok(input.to_uppercase().into())
    }

    fn name(&self) -> &'static str {
        "example::upper"
    }
}
