use std::{path::PathBuf, process::Stdio, sync::Arc};

use tokio::io::AsyncWriteExt;

use crate::modules::{Arg, Command, Module, Ty};

use super::{Context, Input, InputFut};

inventory::submit! {
    Module {
        name: "example",
        commands: &[
            Command {
                name: "reverse", args: &[],
            },
            Command {
                name: "upper", args: &[],
            }
        ]
    }
}

pub async fn reverse(_context: Arc<Context>, input: InputFut) -> anyhow::Result<Input> {
    let input = input.await?.try_into_string().unwrap();
    Ok(input.chars().rev().collect::<String>().into())
}

pub async fn upper(_context: Arc<Context>, input: InputFut) -> anyhow::Result<Input> {
    let input = input.await?.try_into_string().unwrap();
    Ok(input.to_uppercase().into())
}
