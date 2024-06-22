use std::borrow::Cow;
use std::{collections::HashMap, fmt::Display, fmt::Write, sync::Arc};

use crate::modules::CommandRunner;
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
                v.value.as_ref().unwrap_or_else(|| &Value::Null).as_str()
            ))?;
        }
        for (k, v) in args {
            f.write_fmt(format_args!(
                ", {}<{}> = {:?}",
                k,
                v.r#type,
                v.value.as_ref().unwrap_or_else(|| &Value::Null).as_str()
            ))?;
        }
        f.write_char(')')?;
        Ok(())
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Value {
    Int(isize),
    String(String),
    #[default]
    Null,
}

impl Value {
    fn as_str(&self) -> Cow<str> {
        match self {
            Value::Int(x) => Cow::Owned(format!("{}", x)),
            Value::String(x) => Cow::Borrowed(&x),
            Value::Null => Cow::Borrowed("<null>"),
        }
    }

    pub fn try_as_int(&self) -> Option<isize> {
        match self {
            Value::Int(x) => Some(*x),
            _ => None,
        }
    }

    pub fn try_as_string(&self) -> Option<String> {
        match self {
            Value::String(x) => Some(x.clone()),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Arg {
    pub r#type: String,
    pub value_type: Option<String>,
    pub value: Option<Value>,
}

pub struct Pipe {
    context: Arc<Context>,
    modules: HashMap<String, Arc<dyn CommandRunner + Send + Sync>>,
    pub(crate) defn: Arc<PipelineDefinition>,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Command(#[from] crate::modules::Error),
}

impl Pipe {
    #[inline]
    pub fn new(context: Arc<Context>, defn: Arc<PipelineDefinition>) -> Result<Self, Error> {
        let mut cache: HashMap<String, Arc<dyn CommandRunner + Send + Sync>> = HashMap::new();

        for (key, command) in defn.commands.iter() {
            if cache.contains_key(&**key) {
                continue;
            }

            let cmd = (MODULES
                .get(&command.module)
                .unwrap()
                .get(&command.command)
                .unwrap()
                .init)(context.clone(), command.args.clone())?;

            cache.insert(key.clone(), cmd);
        }

        Ok(Self {
            context,
            defn,
            modules: cache,
        })
    }

    #[inline]
    pub async fn forward(
        &self,
        input: Input,
        config: Arc<serde_json::Value>,
    ) -> Result<Input, Error> {
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

                let cmd = Arc::clone(self.modules.get(&**key).unwrap());

                match &command.input {
                    InputValue::Single(x) => {
                        let input = cache.get(&*x.r#ref).unwrap().clone();
                        cache.insert(key, cmd.forward(input, config.clone()).boxed_shared());
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
                        cache.insert(
                            key,
                            cmd.forward(input.boxed_shared(), config.clone())
                                .boxed_shared(),
                        );
                    }
                }
            }
        }

        Ok(cache.remove(&*self.defn.output.r#ref).unwrap().await?)
    }

    pub async fn forward_tap(
        &self,
        input: Input,
        config: Arc<serde_json::Value>,
        tap: fn((usize, usize), &Command, &Input),
    ) -> Result<Input, Error> {
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

                let cmd = Arc::clone(self.modules.get(&**key).unwrap());

                match &command.input {
                    InputValue::Single(x) => {
                        let input = cache.get(&*x.r#ref).unwrap().clone();
                        let tap = tap.clone();
                        let command = command.clone();
                        let config = config.clone();
                        let fut: InputFut = Box::pin(async move {
                            let output = cmd.forward(input, config).await?;
                            tap((i, len), &command, &output);
                            Ok(output)
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
                            Ok(input)
                        });
                        let command = command.clone();
                        let config = config.clone();
                        let output: InputFut = Box::pin(async move {
                            let output = cmd.forward(input.boxed_shared(), config).await?;
                            tap((i, len), &command, &output);
                            Ok(output)
                        });
                        cache.insert(key, output.boxed_shared());
                    }
                }
            }
        }

        Ok(cache.remove(&*self.defn.output.r#ref).unwrap().await?)
    }
}
