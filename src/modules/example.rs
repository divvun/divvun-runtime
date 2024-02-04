use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;

use crate::{
    ast,
    modules::{Command, Module},
};

use super::{CommandRunner, Context, Input, InputFut};

inventory::submit! {
    Module {
        name: "example",
        commands: &[
            Command {
                name: "reverse", args: &[],
                init: Reverse::new,
            },
            Command {
                name: "upper", args: &[],
                init: Upper::new,
            }
        ]
    }
}

pub struct Reverse;

impl Reverse {
    pub fn new(
        _context: Arc<Context>,
        _kwargs: HashMap<String, ast::Arg>,
    ) -> Result<Arc<dyn CommandRunner>, anyhow::Error> {
        Ok(Arc::new(Self) as _)
    }
}

#[async_trait(?Send)]
impl CommandRunner for Reverse {
    async fn forward(self: Arc<Self>, input: InputFut) -> Result<Input, anyhow::Error> {
        let input = input.await?.try_into_string()?;
        Ok(input.chars().rev().collect::<String>().into())
    }
}

pub struct Upper;

impl Upper {
    pub fn new(
        _context: Arc<Context>,
        _kwargs: HashMap<String, ast::Arg>,
    ) -> Result<Arc<dyn CommandRunner>, anyhow::Error> {
        Ok(Arc::new(Self) as _)
    }
}

#[async_trait(?Send)]
impl CommandRunner for Upper {
    async fn forward(self: Arc<Self>, input: InputFut) -> Result<Input, anyhow::Error> {
        let input = input.await?.try_into_string()?;
        Ok(input.to_uppercase().into())
    }
}
