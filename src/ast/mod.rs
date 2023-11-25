use std::{collections::HashMap, path::PathBuf};

use serde::{Deserialize, Serialize};

use crate::modules::InputFut;

#[derive(Debug, Serialize, Deserialize)]
pub struct PipelineDefinition {
    pub ast: Command,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Command {
    #[serde(rename = "command")]
    Command {
        cmd: String,
        #[serde(default)]
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

pub fn from_ast(
    command: Command,
    entry_input: InputFut<String>,
) -> anyhow::Result<InputFut<String>> {
    match command {
        Command::Command {
            cmd,
            mut args,
            input,
        } => match &*cmd {
            "cg3::mwesplit" => {
                return Ok(Box::pin(crate::modules::cg3::mwesplit(from_ast(
                    *input.unwrap(),
                    entry_input,
                )?)))
            }
            "cg3::vislcg3" => {
                let model_path = PathBuf::from(&args.remove("model_path").unwrap().value.unwrap());
                return Ok(Box::pin(crate::modules::cg3::vislcg3(
                    model_path,
                    from_ast(*input.unwrap(), entry_input)?,
                )));
            }
            "divvun::blanktag" => {
                let model_path = PathBuf::from(&args.remove("model_path").unwrap().value.unwrap());
                return Ok(Box::pin(crate::modules::divvun::blanktag(
                    model_path,
                    from_ast(*input.unwrap(), entry_input)?,
                )));
            }
            "divvun::cgspell" => {
                let err_model_path =
                    PathBuf::from(&args.remove("err_model_path").unwrap().value.unwrap());
                let acc_model_path =
                    PathBuf::from(&args.remove("acc_model_path").unwrap().value.unwrap());
                return Ok(Box::pin(crate::modules::divvun::cgspell(
                    err_model_path,
                    acc_model_path,
                    from_ast(*input.unwrap(), entry_input)?,
                )));
            }
            "divvun::suggest" => {
                let model_path = PathBuf::from(&args.remove("model_path").unwrap().value.unwrap());
                let error_xml_path =
                    PathBuf::from(&args.remove("error_xml_path").unwrap().value.unwrap());
                return Ok(Box::pin(crate::modules::divvun::suggest(
                    model_path,
                    error_xml_path,
                    from_ast(*input.unwrap(), entry_input)?,
                )));
            }
            "hfst::tokenize" => {
                let model_path = PathBuf::from(&args.remove("model_path").unwrap().value.unwrap());
                return Ok(Box::pin(crate::modules::hfst::tokenize(
                    model_path,
                    from_ast(*input.unwrap(), entry_input)?,
                )));
            }
            _ => {
                panic!("Unknown command: {}", cmd);
            }
        },
        Command::Entry { type_value: _lol } => Ok(entry_input),
    }
}
