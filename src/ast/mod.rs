use std::{collections::HashMap, path::PathBuf, sync::Arc};

use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};

use crate::{
    modules::{Context, InputFut, Module},
    py::MODULES,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineDefinition {
    pub ast: Command,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Command {
    #[serde(rename = "command")]
    Command {
        module: String,
        command: String,
        #[serde(default)]
        args: HashMap<String, Arg>,
        input: Option<Box<Command>>,
    },
    #[serde(rename = "entry")]
    Entry { type_value: Option<String> },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Arg {
    pub r#type: String,
    pub type_value: Option<String>,
    pub value: Option<String>,
}

// FromAst magic

pub fn from_ast(
    context: Arc<Context>,
    command: Command,
    entry_input: InputFut,
) -> anyhow::Result<InputFut> {
    match command {
        Command::Command {
            module,
            command,
            mut args,
            input,
        } => match &*module {
            "cg3" => match &*command {
                "mwesplit" => Ok(Box::pin(crate::modules::cg3::mwesplit(
                    context.clone(),
                    from_ast(context, *input.unwrap(), entry_input)?,
                ))),
                "vislcg3" => {
                    let model_path =
                        PathBuf::from(&args.remove("model_path").unwrap().value.unwrap());
                    Ok(Box::pin(crate::modules::cg3::vislcg3(
                        context.clone(),
                        model_path,
                        from_ast(context, *input.unwrap(), entry_input)?,
                    )))
                }
                _ => panic!("{:?}", command),
            },
            "divvun" => match &*command {
                "blanktag" => {
                    let model_path =
                        PathBuf::from(&args.remove("model_path").unwrap().value.unwrap());
                    return Ok(Box::pin(crate::modules::divvun::blanktag(
                        context.clone(),
                        model_path,
                        from_ast(context, *input.unwrap(), entry_input)?,
                    )));
                }
                "cgspell" => {
                    let err_model_path =
                        PathBuf::from(&args.remove("err_model_path").unwrap().value.unwrap());
                    let acc_model_path =
                        PathBuf::from(&args.remove("acc_model_path").unwrap().value.unwrap());
                    return Ok(Box::pin(crate::modules::divvun::cgspell(
                        context.clone(),
                        err_model_path,
                        acc_model_path,
                        from_ast(context, *input.unwrap(), entry_input)?,
                    )));
                }
                "suggest" => {
                    let model_path =
                        PathBuf::from(&args.remove("model_path").unwrap().value.unwrap());
                    let error_xml_path =
                        PathBuf::from(&args.remove("error_xml_path").unwrap().value.unwrap());
                    return Ok(Box::pin(crate::modules::divvun::suggest(
                        context.clone(),
                        model_path,
                        error_xml_path,
                        from_ast(context, *input.unwrap(), entry_input)?,
                    )));
                }
                _ => panic!("{:?}", command),
            },
            "hfst" => match &*command {
                "tokenize" => {
                    let model_path =
                        PathBuf::from(&args.remove("model_path").unwrap().value.unwrap());
                    return Ok(Box::pin(crate::modules::hfst::tokenize(
                        context.clone(),
                        model_path,
                        from_ast(context, *input.unwrap(), entry_input)?,
                    )));
                }
                _ => panic!("{:?}", command),
            },
            "example" => match &*command {
                "upper" => Ok(Box::pin(crate::modules::example::upper(
                    context.clone(),
                    from_ast(context, *input.unwrap(), entry_input)?,
                ))),
                "reverse" => Ok(Box::pin(crate::modules::example::reverse(
                    context.clone(),
                    from_ast(context, *input.unwrap(), entry_input)?,
                ))),
                _ => panic!("{:?}", command),
            },
            "speech" => match &*command {
                "tts" => {
                    let voice_model_path =
                        PathBuf::from(&args.remove("voice_model_path").unwrap().value.unwrap());
                    let hifigan_model_path =
                        PathBuf::from(&args.remove("hifigan_model_path").unwrap().value.unwrap());
                    return Ok(Box::pin(crate::modules::speech::tts(
                        context.clone(),
                        from_ast(context, *input.unwrap(), entry_input)?,
                        voice_model_path,
                        hifigan_model_path,
                    )));
                }
                _ => panic!("{:?}", command),
            },
            _ => panic!("{:?}", command),
        },
        Command::Entry { type_value: _lol } => Ok(entry_input),
    }
}
