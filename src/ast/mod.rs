use std::{collections::HashMap, sync::Arc};

use serde::{Deserialize, Serialize};

use crate::{
    modules::{CommandRunner, Context, Input, InputFut, Ty},
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
    Entry { value_type: Option<String> },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Arg {
    pub r#type: String,
    pub value_type: Option<String>,
    pub value: Option<String>,
}

pub struct Pipe {
    entry_type: Ty,
    commands: Vec<Arc<dyn CommandRunner>>,
}

impl Pipe {
    pub async fn forward(&self, input: Input) -> Result<Input, anyhow::Error> {
        let mut input: InputFut = Box::pin(async { Ok(input) });
        let commands = self.commands.clone();

        for command in commands.iter().cloned() {
            input = command.forward(input);
        }

        input.await
    }

    pub async fn forward_tap<F: Fn(Arc<dyn CommandRunner>, &Input) + 'static>(
        &self,
        input: Input,
        tap: Arc<F>,
    ) -> Result<Input, anyhow::Error> {
        let mut input: InputFut = Box::pin(async { Ok(input) });
        let commands = self.commands.clone();

        for command in commands.iter().cloned() {
            let tap = tap.clone();
            input = Box::pin(async move {
                let cmd = command.clone();
                let output = command.forward(input).await?;
                tap(cmd, &output);
                Ok(output)
            }) as _;
        }

        input.await
    }
}

pub fn from_ast(context: Arc<Context>, command: Command) -> anyhow::Result<Pipe> {
    let (mut commands, entry_type) = _from_ast(context, command, vec![], None)?;
    commands.reverse();

    let Some(entry_type) = entry_type else {
        anyhow::bail!("Missing entry type");
    };

    let entry_type = match entry_type.as_str() {
        "path" => Ty::Path,
        "string" => Ty::String,
        _ => {
            anyhow::bail!("Unsupported entry type: {}", entry_type)
        }
    };

    Ok(Pipe {
        entry_type,
        commands,
    })
}

fn _from_ast(
    context: Arc<Context>,
    command: Command,
    mut commands: Vec<Arc<dyn CommandRunner>>,
    entry_type: Option<String>,
) -> anyhow::Result<(Vec<Arc<dyn CommandRunner>>, Option<String>)> {
    match command {
        Command::Command {
            module,
            command,
            args,
            input,
        } => {
            let cmd =
                (MODULES.get(&module).unwrap().get(&command).unwrap().init)(context.clone(), args)?;
            commands.push(cmd);
            _from_ast(context, *input.unwrap(), commands, entry_type)
        }
        Command::Entry { value_type } => Ok((commands, value_type)),
    }
}
