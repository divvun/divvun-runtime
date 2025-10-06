use std::any::Any;
use std::path::PathBuf;
use std::pin::Pin;
use std::{collections::HashMap, fmt::Display, sync::Arc};

use crate::modules::{CommandRunner, InputEvent, InputRx, InputTx, Tap, TapFn};
use futures_util::Stream;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, broadcast};
use tokio::task::JoinHandle;

use crate::{
    modules::{Context, Input},
    ts::MODULES,
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

impl PipelineDefinition {
    pub fn assets(&self) -> Vec<PathBuf> {
        self.commands
            .values()
            .map(|cmd| {
                cmd.args
                    .values()
                    .cloned()
                    .filter(|x| {
                        // Strictly speaking, this is a hack and should be parsed properly
                        x.r#type.contains("path")
                    })
                    .filter_map(|x| x.value)
                    .map(|x| {
                        if let Some(v) = x.try_as_map_path() {
                            v.into_iter().map(|(_, v)| v).collect::<Vec<_>>()
                        } else if let Some(v) = x.try_as_array_path() {
                            v
                        } else if let Some(v) = x.try_as_path() {
                            vec![v]
                        } else {
                            vec![]
                        }
                    })
                    .flatten()
                    .collect::<Vec<_>>()
            })
            .flatten()
            .collect::<Vec<_>>()
    }
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
}

impl Display for Command {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Module in green, command in cyan
        f.write_fmt(format_args!(
            "\x1b[32m{}\x1b[0m::\x1b[36m{}\x1b[0m(",
            self.module, self.command
        ))?;

        let mut args = self.args.iter();
        let arg = args.next();
        if let Some((k, v)) = arg {
            // Argument name in yellow, type in grey, value in bold white
            f.write_fmt(format_args!(
                "{} = \x1b[90m<{}>\x1b[0m\x1b[1m{:#}\x1b[0m",
                k,
                v.r#type,
                v.value.as_ref().unwrap_or_else(|| &Value::Null)
            ))?;
        }
        for (k, v) in args {
            // Same colors for subsequent args
            f.write_fmt(format_args!(
                ", {} = \x1b[90m<{}>\x1b[0m\x1b[1m{:#}\x1b[0m",
                k,
                v.r#type,
                v.value.as_ref().unwrap_or_else(|| &Value::Null)
            ))?;
        }
        f.write_fmt(format_args!(") \x1b[90m-> {}\x1b[0m", self.returns))?;
        Ok(())
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Value {
    Int(isize),
    String(String),
    Array(Vec<Value>),
    Map(IndexMap<String, Value>),
    #[default]
    Null,
}

impl Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::Int(x) => write!(f, "\x1b[1;32m{}\x1b[0m", x),
            Value::String(x) => write!(f, "\x1b[1;31m{:?}\x1b[0m", x),
            Value::Array(x) => {
                write!(f, "\x1b[1;37m[\x1b[0m")?;
                for (i, x) in x.iter().enumerate() {
                    if i > 0 {
                        write!(f, "\x1b[1;37m, \x1b[0m")?;
                    }
                    x.fmt(f)?;
                }
                write!(f, "\x1b[1;37m]\x1b[0m")?;
                Ok(())
            }
            Value::Map(x) => {
                write!(f, "\x1b[1;37m{{")?;
                for (i, (k, v)) in x.iter().enumerate() {
                    if i > 0 {
                        write!(f, "\x1b[1;37m, \x1b[0m")?;
                    }
                    write!(f, "{}: {}", k, v)?;
                }
                write!(f, "\x1b[1;37m}}\x1b[0m")?;
                Ok(())
            }
            Value::Null => write!(f, "\x1b[1;90m␀\x1b[0m"),
        }
    }
}

impl Value {
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

    pub fn try_as_path(&self) -> Option<PathBuf> {
        match self {
            Value::String(x) => Some(PathBuf::from(x)),
            _ => None,
        }
    }

    pub fn try_as_array_string(&self) -> Option<Vec<String>> {
        match self {
            Value::Array(x) => Some(
                x.iter()
                    .map(|x| x.try_as_string())
                    .collect::<Option<Vec<_>>>()?,
            ),
            _ => None,
        }
    }

    pub fn try_as_array_path(&self) -> Option<Vec<PathBuf>> {
        match self {
            Value::Array(x) => Some(
                x.iter()
                    .map(|x| x.try_as_path())
                    .collect::<Option<Vec<_>>>()?,
            ),
            _ => None,
        }
    }

    pub fn try_as_map_path(&self) -> Option<IndexMap<String, PathBuf>> {
        match self {
            Value::Map(x) => Some(
                x.iter()
                    .map(|(k, v)| (k.clone(), v.try_as_path().unwrap()))
                    .collect(),
            ),
            _ => None,
        }
    }

    pub fn try_as_map_string(&self) -> Option<IndexMap<String, String>> {
        match self {
            Value::Map(x) => Some(
                x.iter()
                    .map(|(k, v)| (k.clone(), v.try_as_string().unwrap()))
                    .collect(),
            ),
            _ => None,
        }
    }

    pub fn try_as_json(&self) -> Result<serde_json::Value, serde_json::Error> {
        match self {
            Value::Int(x) => Ok(serde_json::Value::Number(serde_json::Number::from(*x))),
            Value::String(x) => Ok(serde_json::Value::String(x.clone())),
            Value::Array(x) => Ok(serde_json::Value::Array(
                x.iter()
                    .map(|x| x.try_as_json())
                    .collect::<Result<Vec<_>, _>>()?,
            )),
            Value::Map(x) => Ok(serde_json::Value::Object(
                x.iter()
                    .map(|(k, v)| Ok((k.clone(), v.try_as_json()?)))
                    .collect::<Result<serde_json::Map<String, serde_json::Value>, _>>()?,
            )),
            Value::Null => Ok(serde_json::Value::Null),
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
    _context: Arc<Context>,
    modules: IndexMap<String, Arc<dyn CommandRunner + Send + Sync>>,
    pub(crate) defn: Arc<PipelineDefinition>,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Command(#[from] crate::modules::Error),
}

pub struct PipelineHandle {
    handles: Vec<JoinHandle<Result<(), crate::modules::Error>>>,
    input: Arc<Mutex<InputTx>>,
    output: InputRx,
}

impl Drop for PipelineHandle {
    fn drop(&mut self) {
        let _ = self
            .input
            .try_lock()
            .map(|x| x.send(InputEvent::Close))
            .unwrap();
        for handle in self.handles.iter() {
            handle.abort();
        }
        self.handles.clear();
    }
}

type PipelineStream =
    Pin<Box<dyn Stream<Item = Result<Input, crate::modules::Error>> + Send + 'static>>;

impl PipelineHandle {
    pub async fn forward(&mut self, input: Input) -> PipelineStream {
        let input_lock = Arc::clone(&self.input);
        let mut rx = self.output.resubscribe();

        let output = Box::pin(async_stream::stream! {
            let guard = input_lock.lock().await;
            match guard.send(InputEvent::Input(input)) {
                Ok(_) => (),
                Err(e) => {
                    yield Err(crate::modules::Error(e.to_string()));
                    return;
                }
            }

            loop {
                match rx.recv().await {
                    Ok(InputEvent::Input(input)) => {
                        yield Ok(input)
                    },
                    Ok(InputEvent::Error(e)) => {
                        yield Err(e);
                        break;
                    },
                    Ok(InputEvent::Close) => {
                        break;
                    }
                    Ok(InputEvent::Finish) => {
                        break;
                    }
                    Err(e) => yield Err(crate::modules::Error(e.to_string())),
                }
            }
        });

        output
    }
}

impl Pipe {
    #[inline]
    pub fn new(context: Arc<Context>, defn: Arc<PipelineDefinition>) -> Result<Self, Error> {
        let mut cache: IndexMap<String, Arc<dyn CommandRunner + Send + Sync>> = IndexMap::new();

        for (key, command) in defn.commands.iter() {
            if cache.contains_key(&**key) {
                continue;
            }

            let module =
                MODULES
                    .get(&command.module)
                    .ok_or(Error::Command(crate::modules::Error(format!(
                        "Module {} not found",
                        command.module
                    ))))?;
            let subcommand =
                module
                    .get(&command.command)
                    .ok_or(Error::Command(crate::modules::Error(format!(
                        "Module {}, command {} not found",
                        command.module, command.command
                    ))))?;
            let cmd =
                (subcommand.init)(context.clone(), command.args.clone()).map_err(Error::Command)?;

            cache.insert(key.clone(), cmd);
        }

        Ok(Self {
            _context: context,
            defn,
            modules: cache,
        })
    }

    pub fn command<T: CommandRunner>(&self, key: &str) -> Option<&T> {
        self.modules
            .get(key)
            .map(|x| &**x as &(dyn Any + Send + Sync))
            .and_then(|x| x.downcast_ref::<T>())
    }

    pub async fn create_stream(
        &self,
        config: Arc<serde_json::Value>,
        tap: Option<Arc<TapFn>>,
    ) -> Result<PipelineHandle, Error> {
        let (main_input_tx, _main_input_rx) = broadcast::channel(16);
        let mut cache: IndexMap<&str, InputTx> = IndexMap::new();
        let mut outputs: HashMap<&str, InputRx> = HashMap::new();
        let mut handles: HashMap<&str, JoinHandle<Result<(), crate::modules::Error>>> =
            HashMap::new();

        cache.insert("#/entry", main_input_tx.clone());
        let output_ref = &*self.defn.output.r#ref;

        while !cache.contains_key(output_ref) {
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
                        let parent_input = cache.get(&*x.r#ref).unwrap().clone();
                        let parent_output = parent_input.subscribe();
                        let (child_input, child_output) = broadcast::channel::<InputEvent>(16);

                        let tap = tap.clone().map(|x| Tap {
                            key: key.to_string().into(),
                            command: Arc::new(command.clone()),
                            tap: x,
                        });
                        // Extract command-specific config or use empty object
                        let cmd_config = config
                            .as_object()
                            .and_then(|obj| obj.get(key))
                            .map(|v| Arc::new(v.clone()))
                            .unwrap_or_else(|| Arc::new(serde_json::Value::Null));

                        let handle =
                            cmd.forward_stream(parent_output, child_input.clone(), tap, cmd_config);
                        handles.insert(key, handle);
                        cache.insert(key, child_input);
                        outputs.insert(key, child_output);

                        if output_ref == *key {
                            break;
                        }
                    }
                    InputValue::Multiple(x) => {
                        todo!()
                        // let inputs = x
                        //     .iter()
                        //     .map(|x| cache.get(&*x.r#ref).unwrap().clone())
                        //     .collect::<Vec<_>>();
                        // let fut = join_all(inputs.into_iter());
                        // let input: InputFut = Box::pin(async move {
                        //     Ok(Input::Multiple(
                        //         fut.await
                        //             .into_iter()
                        //             .collect::<Result<Vec<_>, _>>()?
                        //             .into_boxed_slice(),
                        //     ))
                        // });
                        // cache.insert(
                        //     key,
                        //     cmd.forward(input.boxed_shared(), config.clone())
                        //         .boxed_shared(),
                        // );
                    }
                }
            }
        }

        let main_output_rx = outputs.remove(output_ref).unwrap();

        Ok(PipelineHandle {
            handles: handles.into_values().collect(),
            input: Arc::new(Mutex::new(main_input_tx)),
            output: main_output_rx,
        })
    }
}
