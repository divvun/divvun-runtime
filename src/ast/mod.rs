use std::{collections::HashMap, fmt::Display, fmt::Write, sync::Arc};

use crate::{modules::SharedInputFut, util::FutureExt as _};
use futures_util::future::join_all;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use crate::{
    modules::{Context, Input, InputFut},
    py::MODULES,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ref {
    pub r#ref: String,
}

impl Ref {
    pub fn resolve<'a>(&self, defn: &'a PipelineDefinition) -> Option<&'a Command> {
        defn.commands.get(&self.r#ref)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    pub value_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineDefinition {
    pub entry: Entry,
    pub output: Ref,
    pub commands: IndexMap<String, Command>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum InputValue {
    Single(Ref),
    Multiple(Vec<Ref>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Command {
    pub module: String,
    pub command: String,
    #[serde(default)]
    pub args: HashMap<String, Arg>,
    pub input: InputValue,
    pub returns: String,
}

impl Display for Command {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("{}::{}(", self.module, self.command))?;
        let mut args = self.args.iter();
        let arg = args.next();
        if let Some((k, v)) = arg {
            f.write_fmt(format_args!(
                "{}<{}> = {:?}",
                k,
                v.r#type,
                v.value.as_deref().unwrap_or("<null>")
            ))?;
        }
        for (k, v) in args {
            f.write_fmt(format_args!(
                ", {}<{}> = {:?}",
                k,
                v.r#type,
                v.value.as_deref().unwrap_or("<null>")
            ))?;
        }
        f.write_char(')')?;
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Arg {
    pub r#type: String,
    pub value_type: Option<String>,
    pub value: Option<String>,
}

pub struct Pipe {
    context: Arc<Context>,
    pub(crate) defn: Arc<PipelineDefinition>,
}

impl Pipe {
    #[inline]
    pub fn new(context: Arc<Context>, defn: Arc<PipelineDefinition>) -> Self {
        Self { context, defn }
    }

    #[inline]
    pub async fn forward(&self, input: Input) -> Result<Input, Arc<anyhow::Error>> {
        let input_fut: InputFut = Box::pin(async { Ok(input) });
        let mut cache: HashMap<&str, SharedInputFut> = HashMap::new();
        cache.insert("#/entry", input_fut.boxed_shared());

        while !cache.contains_key(&*self.defn.output.r#ref) {
            for (key, command) in self.defn.commands.iter() {
                if cache.contains_key(&**key) {
                    continue;
                }

                match &command.input {
                    InputValue::Single(x) => {
                        if !cache.contains_key(&*x.r#ref) {
                            continue;
                        }
                    }
                    InputValue::Multiple(x) => {
                        if !x.iter().all(|x| cache.contains_key(&*x.r#ref)) {
                            continue;
                        }
                    }
                }

                let cmd = (MODULES
                    .get(&command.module)
                    .unwrap()
                    .get(&command.command)
                    .unwrap()
                    .init)(self.context.clone(), command.args.clone())?;

                match &command.input {
                    InputValue::Single(x) => {
                        let input = cache.get(&*x.r#ref).unwrap().clone();
                        cache.insert(key, cmd.forward(input).boxed_shared());
                    }
                    InputValue::Multiple(x) => {
                        let inputs = x
                            .iter()
                            .map(|x| cache.get(&*x.r#ref).unwrap().clone())
                            .collect::<Vec<_>>();
                        let fut = join_all(inputs.into_iter());
                        let input: InputFut = Box::pin(async move {
                            Ok(Input::Multiple(
                                fut.await
                                    .into_iter()
                                    .collect::<Result<Vec<_>, _>>()?
                                    .into_boxed_slice(),
                            ))
                        });
                        cache.insert(key, cmd.forward(input.boxed_shared()).boxed_shared());
                    }
                }
            }
        }

        cache.remove(&*self.defn.output.r#ref).unwrap().await
    }

    pub async fn forward_tap(
        &self,
        input: Input,
        tap: fn((usize, usize), &Command, &Input),
    ) -> Result<Input, Arc<anyhow::Error>> {
        let input_fut: InputFut = Box::pin(async { Ok(input) });
        let mut cache: HashMap<&str, SharedInputFut> = HashMap::new();
        cache.insert("#/entry", input_fut.boxed_shared());

        let len = self.defn.commands.len();

        while !cache.contains_key(&*self.defn.output.r#ref) {
            for (i, (key, command)) in self.defn.commands.iter().enumerate() {
                if cache.contains_key(&**key) {
                    continue;
                }

                match &command.input {
                    InputValue::Single(x) => {
                        if !cache.contains_key(&*x.r#ref) {
                            continue;
                        }
                    }
                    InputValue::Multiple(x) => {
                        if !x.iter().all(|x| cache.contains_key(&*x.r#ref)) {
                            continue;
                        }
                    }
                }

                let cmd = (MODULES
                    .get(&command.module)
                    .unwrap()
                    .get(&command.command)
                    .unwrap()
                    .init)(self.context.clone(), command.args.clone())?;

                match &command.input {
                    InputValue::Single(x) => {
                        let input = cache.get(&*x.r#ref).unwrap().clone();
                        let tap = tap.clone();
                        let command = command.clone();
                        let fut: InputFut = Box::pin(async move {
                            let output = cmd.forward(input).await?;
                            tap((i, len), &command, &output);
                            Ok::<_, Arc<anyhow::Error>>(output)
                        });
                        cache.insert(key, fut.boxed_shared());
                    }
                    InputValue::Multiple(x) => {
                        let inputs = x
                            .iter()
                            .map(|x| cache.get(&*x.r#ref).unwrap().clone())
                            .collect::<Vec<_>>();
                        let fut = join_all(inputs.into_iter());
                        let tap = tap.clone();
                        let input: InputFut = Box::pin(async move {
                            let input = Input::Multiple(
                                fut.await
                                    .into_iter()
                                    .collect::<Result<Vec<_>, _>>()?
                                    .into_boxed_slice(),
                            );
                            Ok::<_, Arc<anyhow::Error>>(input)
                        });
                        let command = command.clone();
                        let output: InputFut = Box::pin(async move {
                            let output = cmd.forward(input.boxed_shared()).await?;
                            tap((i, len), &command, &output);
                            Ok(output)
                        });
                        cache.insert(key, output.boxed_shared());
                    }
                }
            }
        }

        cache.remove(&*self.defn.output.r#ref).unwrap().await
    }
}
