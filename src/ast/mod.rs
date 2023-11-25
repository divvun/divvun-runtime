use std::{
    collections::HashMap,
    future::{Future, IntoFuture},
    path::{Path, PathBuf},
    pin::Pin,
};

use serde::{Deserialize, Serialize};

use crate::modules::InputFut;

#[derive(Debug, Serialize, Deserialize)]
pub struct PipelineDefinition {
    pub ast: Command,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Command {
    #[serde(rename = "command")]
    Command {
        cmd: String,
        args: HashMap<String, Arg>,
        input: Option<Box<Command>>,
    },
    #[serde(rename = "entry")]
    Entry { type_value: Option<String> },
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Arg {
    pub r#type: String,
    pub type_value: Option<String>,
    pub value: Option<String>,
}

// FromAst magic

fn from_ast(command: Command) -> anyhow::Result<InputFut<String>> {
    match command {
        Command::Command {
            cmd,
            mut args,
            input,
        } => match &*cmd {
            "cg3::vislcg3" => {
                let model_path = PathBuf::from(&args.remove("model_path").unwrap().value.unwrap());
                return Ok(Box::pin(
                    crate::modules::cg3::vislcg3(model_path, from_ast(*input.unwrap())?)
                        .into_future(),
                ));
            }
            "divvun::blanktag" => {
                let model_path = PathBuf::from(&args.remove("model_path").unwrap().value.unwrap());
                return Ok(Box::pin(
                    crate::modules::divvun::blanktag(model_path, from_ast(*input.unwrap())?)
                        .into_future(),
                ));
            }
            _ => {
                panic!("Unknown command: {}", cmd);
            }
        },
        Command::Entry { type_value: _lol } => Ok(Box::pin(async {
            Ok("This is an example string".to_string())
        })),
    }
}
