use std::{
    collections::{HashMap, VecDeque},
    ops::Deref,
    pin::Pin,
    sync::Arc,
};

use crate::{modules::SharedInputFut, util::FutureExt as _};
use futures_util::FutureExt as _;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use crate::{
    modules::{CommandRunner, Context, Input, InputFut, Ty},
    py::MODULES,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ref {
    r#ref: String,
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
    module: String,
    command: String,
    #[serde(default)]
    args: HashMap<String, Arg>,
    input: InputValue,
    returns: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Arg {
    pub r#type: String,
    pub value_type: Option<String>,
    pub value: Option<String>,
}

pub struct Pipe {
    context: Arc<Context>,
    defn: Arc<PipelineDefinition>,
}

impl Pipe {
    pub fn new(context: Arc<Context>, defn: Arc<PipelineDefinition>) -> Self {
        Self { context, defn }
    }
    
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
                        let fut = futures_util::future::join_all(inputs.into_iter());
                        let input: InputFut = Box::pin(async move {
                            Ok(Input::Multiple(fut.await.into_iter().collect::<Result<Vec<_>, _>>()?.into_boxed_slice()))
                        });
                        cache.insert(key, cmd.forward(input.boxed_shared()).boxed_shared());
                    }
                }
            }
        }

        cache.remove(&*self.defn.output.r#ref).unwrap().await
    }

    // pub async fn forward_tap<F: Fn(Arc<dyn CommandRunner>, &Input) + 'static>(
    //     &self,
    //     input: Input,
    //     tap: Arc<F>,
    // ) -> Result<Input, Arc<anyhow::Error>> {
    //     let commands = CommandValue::Multiple(self.commands.clone().into_boxed_slice().into());
    //     Self::_forward_tap(commands, input, tap).await
    // }
}
