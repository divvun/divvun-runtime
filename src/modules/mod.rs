use std::{
    any::Any,
    borrow::Cow,
    collections::HashMap,
    fmt::{Display, Write},
    future::Future,
    io::Read,
    path::{Path, PathBuf},
    pin::Pin,
    str::FromStr,
    sync::Arc,
};

use once_cell::sync::Lazy;

use async_trait::async_trait;
use bitmask_enum::bitmask;
use box_format::{BoxFileReader, BoxPath, Compression};
use memmap2::Mmap;
use tempfile::TempDir;
use tokio::{
    sync::broadcast::{Receiver, Sender},
    task::JoinHandle,
};

use crate::{
    ast::{self, Command, PipelineDefinition},
    util::SharedBox,
};

// Simple glob matching for patterns like "errors-*.ftl"
fn glob_match(pattern: &str, text: &str) -> bool {
    // Simple implementation for patterns like "errors-*.ftl"
    if let Some(star_pos) = pattern.find('*') {
        let prefix = &pattern[..star_pos];
        let suffix = &pattern[star_pos + 1..];
        text.starts_with(prefix) && text.ends_with(suffix)
    } else {
        pattern == text
    }
}

#[cfg(feature = "mod-cg3")]
pub mod cg3;
pub mod debug;
#[cfg(feature = "mod-divvun")]
pub mod divvun;
pub mod example;
#[cfg(feature = "mod-hfst")]
pub mod hfst;
pub mod runtime;
#[cfg(feature = "mod-speech")]
pub mod speech;
pub mod spell;
#[cfg(feature = "mod-ssml")]
pub mod ssml;

pub type InputFut = Pin<Box<dyn Future<Output = Result<Input, Error>> + Send>>;
pub type SharedInputFut = SharedBox<dyn Future<Output = Result<Input, Error>> + Send>;

#[derive(Debug, Clone)]
pub enum InputEvent {
    Input(Input),
    Error(Error),
    Finish,
    Close,
}

pub type InputTx = Sender<InputEvent>;
pub type InputRx = Receiver<InputEvent>;

#[derive(Debug, Clone)]
pub enum Input {
    String(String),
    Bytes(Vec<u8>),
    ArrayString(Vec<String>),
    ArrayBytes(Vec<Vec<u8>>),
    Json(serde_json::Value),
    Multiple(Box<[Input]>),
}

impl Display for InputEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InputEvent::Input(x) => write!(f, "{:#}", x)?,
            InputEvent::Error(x) => write!(f, "Error: {:#}", x)?,
            InputEvent::Finish => write!(f, "Finish")?,
            InputEvent::Close => write!(f, "Close")?,
        }
        Ok(())
    }
}

impl Display for Input {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if f.alternate() {
            match self {
                Input::String(x) => write!(f, "{}", x)?,
                Input::Bytes(x) => write!(f, "<<{} bytes>>", x.len())?,
                Input::ArrayString(x) => write!(f, "{:#?}", x)?,
                Input::ArrayBytes(x) => {
                    write!(f, "[")?;
                    for (_i, x) in x.iter().enumerate() {
                        write!(f, "<<{} bytes>>,", x.len())?;
                    }
                    write!(f, "]")?;
                }
                Input::Json(x) => write!(f, "{}", serde_json::to_string_pretty(&x).unwrap())?,
                Input::Multiple(x) => {
                    writeln!(f, "[")?;
                    for (i, x) in x.iter().enumerate() {
                        writeln!(f, "{i}: {}", x)?;
                    }
                    write!(f, "]")?;
                }
            }
            return Ok(());
        }

        match self {
            Input::String(x) => write!(f, "{}", x),
            Input::Bytes(x) => write!(f, "<<{} bytes>>", x.len()),
            Input::ArrayString(x) => write!(f, "[{}]", x.join(", ")),
            Input::ArrayBytes(x) => write!(
                f,
                "[{}]",
                x.iter()
                    .map(|x| format!("<<{} bytes>>", x.len()))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Input::Json(x) => write!(f, "{}", serde_json::to_string(&x).unwrap()),
            Input::Multiple(x) => write!(
                f,
                "[{}]",
                x.iter()
                    .map(|x| format!("{}", x))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        }
    }
}

#[derive(Clone, Debug, thiserror::Error)]
#[error("{0}")]
pub struct Error(pub String);

impl Input {
    pub fn try_into_string(self) -> Result<String, Error> {
        match self {
            Input::String(x) => Ok(x),
            _ => Err(Error("Could not convert input to string".to_string())),
        }
    }

    pub fn try_into_bytes(self) -> Result<Vec<u8>, Error> {
        match self {
            Input::Bytes(x) => Ok(x),
            _ => Err(Error("Could not convert input to bytes".to_string())),
        }
    }

    pub fn try_into_json(self) -> Result<serde_json::Value, Error> {
        match self {
            Input::Json(x) => Ok(x),
            _ => Err(Error("Could not convert input to json".to_string())),
        }
    }

    pub fn try_into_string_array(self) -> Result<Vec<String>, Error> {
        match self {
            Input::ArrayString(x) => Ok(x),
            _ => Err(Error("Could not convert input to string array".to_string())),
        }
    }

    pub fn try_into_bytes_array(self) -> Result<Vec<Vec<u8>>, Error> {
        match self {
            Input::ArrayBytes(x) => Ok(x),
            _ => Err(Error("Could not convert input to bytes array".to_string())),
        }
    }

    pub fn try_into_multiple(self) -> Result<Box<[Input]>, Error> {
        match self {
            Input::Multiple(x) => Ok(x),
            _ => Err(Error("Could not convert input to multiple".to_string())),
        }
    }
}

impl From<String> for Input {
    fn from(value: String) -> Self {
        Input::String(value)
    }
}

impl From<Vec<u8>> for Input {
    fn from(value: Vec<u8>) -> Self {
        Input::Bytes(value)
    }
}

impl From<serde_json::Value> for Input {
    fn from(value: serde_json::Value) -> Self {
        Input::Json(value)
    }
}

impl From<Vec<String>> for Input {
    fn from(value: Vec<String>) -> Self {
        Input::ArrayString(value)
    }
}

pub enum DataRef {
    BoxFile(Box<BoxFileReader>, TempDir),
    Path(PathBuf),
}

pub struct Context {
    pub(crate) data: DataRef,
}

impl Context {
    pub fn load_pipeline_definition(&self) -> Result<PipelineDefinition, Error> {
        match &self.data {
            DataRef::BoxFile(bf, _) => {
                let record = bf
                    .find(&BoxPath::new("pipeline.json").map_err(|e| Error(e.to_string()))?)
                    .map_err(|e| Error(e.to_string()))?
                    .as_file()
                    .unwrap();
                let pipeline: PipelineDefinition = if record.compression == Compression::Stored {
                    let m = unsafe { bf.memory_map(&record) }.map_err(|e| Error(e.to_string()))?;
                    serde_json::from_reader(&*m).map_err(|e| Error(e.to_string()))?
                } else {
                    let mut buf = Vec::with_capacity(record.decompressed_length as _);
                    let m = bf
                        .read_bytes(record)
                        .map_err(|e| Error(e.to_string()))?
                        .read_to_end(&mut buf);
                    serde_json::from_slice(&buf).map_err(|e| Error(e.to_string()))?
                };
                Ok(pipeline)
            }
            DataRef::Path(p) => {
                let p = p.join("pipeline.json");
                let f = std::fs::File::open(p).map_err(|e| Error(e.to_string()))?;
                let m = unsafe { Mmap::map(&f) }.map_err(|e| Error(e.to_string()))?;
                let pipeline: PipelineDefinition =
                    serde_json::from_reader(&*m).map_err(|e| Error(e.to_string()))?;
                Ok(pipeline)
            }
        }
    }

    pub fn load_file(&self, path: impl AsRef<Path>) -> Result<impl Read, Error> {
        match &self.data {
            DataRef::BoxFile(bf, _) => {
                tracing::debug!("Loading file from box file: {}", path.as_ref().display());
                let record = bf
                    .find(&BoxPath::new(path).map_err(|e| Error(e.to_string()))?)
                    .map_err(|e| Error(e.to_string()))?
                    .as_file()
                    .unwrap();
                let out = bf.read_bytes(record).map_err(|e| Error(e.to_string()))?;
                Ok(out)
            }
            DataRef::Path(p) => {
                println!(
                    "Loading file from path: {}",
                    p.join("assets").join(&path).display()
                );
                let out = std::fs::File::open(p.join("assets").join(path))
                    .map_err(|e| Error(e.to_string()))?;
                Ok(out.take(u64::MAX))
            }
        }
    }

    pub fn extract_to_temp_dir(&self, path: impl AsRef<Path>) -> Result<PathBuf, Error> {
        match &self.data {
            DataRef::BoxFile(bf, tmp) => {
                tracing::debug!("Extracting file to temp dir: {}", path.as_ref().display());
                let bpath = BoxPath::new(path.as_ref()).map_err(|e| Error(e.to_string()))?;
                bf.extract_recursive(&bpath, tmp.path())
                    .map_err(|e| Error(e.to_string()))?;
                Ok(tmp.path().join(path.as_ref()))
            }
            DataRef::Path(p) => {
                tracing::debug!(
                    "Extracting file to temp dir: {}",
                    p.join("assets").join(&path).display()
                );
                Ok(p.join("assets").join(path))
            }
        }
    }

    pub fn load_files_glob(&self, pattern: &str) -> Result<Vec<(PathBuf, Box<dyn Read>)>, Error> {
        match &self.data {
            DataRef::BoxFile(bf, _tmp) => {
                // For box files, we need to iterate through entries and match the pattern
                let mut files = Vec::new();
                for entry in bf.metadata().iter() {
                    let path_str = entry.path.to_string();
                    if glob_match(pattern, &path_str) {
                        if let Some(file_record) = entry.record.as_file() {
                            let reader = bf
                                .read_bytes(file_record)
                                .map_err(|e| Error(e.to_string()))?;
                            files
                                .push((PathBuf::from(path_str), Box::new(reader) as Box<dyn Read>));
                        }
                    }
                }
                Ok(files)
            }
            DataRef::Path(p) => {
                // For regular paths, use filesystem globbing
                let assets_dir = p.join("assets");
                let full_pattern = assets_dir.join(pattern);
                let mut files = Vec::new();

                for entry in
                    glob::glob(full_pattern.to_str().unwrap()).map_err(|e| Error(e.to_string()))?
                {
                    let path = entry.map_err(|e| Error(e.to_string()))?;
                    if path.is_file() {
                        let file = std::fs::File::open(&path).map_err(|e| Error(e.to_string()))?;
                        files.push((path, Box::new(file) as Box<dyn Read>));
                    }
                }
                Ok(files)
            }
        }
    }

    pub fn memory_map_file(&self, path: impl AsRef<Path>) -> Result<Mmap, Error> {
        match &self.data {
            DataRef::BoxFile(bf, _tmp) => {
                tracing::debug!("Memory mapping file: {}", path.as_ref().display());
                let bpath = BoxPath::new(path.as_ref()).map_err(|e| Error(e.to_string()))?;
                let node = bf.find(&bpath).map_err(|e| Error(e.to_string()))?;
                let node = node
                    .as_file()
                    .ok_or_else(|| Error("Not a file".to_string()))?;
                Ok(unsafe { bf.memory_map(node).map_err(|e| Error(e.to_string()))? })
            }
            DataRef::Path(p) => {
                tracing::debug!(
                    "Memory mapping file: {}",
                    p.join("assets").join(&path).display()
                );
                let f = std::fs::File::open(p.join("assets").join(path))
                    .map_err(|e| Error(e.to_string()))?;
                Ok(unsafe { Mmap::map(&f).map_err(|e| Error(e.to_string()))? })
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Module {
    pub name: &'static str,
    pub commands: &'static [CommandDef],
}

impl Display for Module {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Module: ")?;
        f.write_str(self.name)?;
        f.write_str("\n")?;
        for command in self.commands {
            f.write_str("  ")?;
            f.write_str(command.name)?;
            f.write_char('\n')?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct CommandDef {
    pub name: &'static str,
    pub module: &'static str,
    pub input: &'static [Ty],
    pub args: &'static [Arg],
    pub assets: &'static [AssetDep],
    pub init: fn(
        Arc<Context>,
        HashMap<String, ast::Arg>,
    ) -> Result<Arc<dyn CommandRunner + Send + Sync>, Error>,
    pub returns: Ty,
}

#[derive(Debug, Clone)]
pub struct Arg {
    pub name: &'static str,
    pub ty: Ty,
    pub optional: bool,
}

#[derive(Debug, Clone)]
pub struct StructDef {
    pub name: &'static str,
    pub module: &'static str,
    pub fields: &'static [StructField],
}

#[derive(Debug, Clone)]
pub struct StructField {
    pub name: &'static str,
    pub ty: &'static str,
    pub optional: bool,
}

#[derive(Debug, Clone)]
pub enum AssetDep {
    Required(&'static str),      // required("file.json")
    RequiredRegex(&'static str), // required(r"pattern")
    Optional(&'static str),      // optional("file.json")
    OptionalRegex(&'static str), // optional(r"pattern")
}

#[derive(Debug, Clone)]
pub enum Ty {
    Path,
    String,
    Json,
    Bytes,
    Int,
    ArrayString,
    ArrayBytes,
    MapPath,
    MapString,
    MapBytes,
    Struct(&'static str), // Custom struct type with name
    Union(Vec<Ty>),       // For supporting multiple types (replacing bitmask functionality)
}

impl FromStr for Ty {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.starts_with('[') && s.ends_with(']') {
            let inner = Ty::from_str(&s[1..s.len() - 1])?;
            return if matches!(inner, Ty::String) {
                Ok(Ty::ArrayString)
            } else if matches!(inner, Ty::Bytes) {
                Ok(Ty::ArrayBytes)
            } else {
                Err(())
            };
        }

        if s.starts_with("{") && s.ends_with("}") {
            let inner = Ty::from_str(&s[1..s.len() - 1])?;
            return if matches!(inner, Ty::Path) {
                Ok(Ty::MapPath)
            } else if matches!(inner, Ty::String) {
                Ok(Ty::MapString)
            } else if matches!(inner, Ty::Bytes) {
                Ok(Ty::MapBytes)
            } else {
                Err(())
            };
        }

        match s {
            "path" => Ok(Ty::Path),
            "string" => Ok(Ty::String),
            "json" => Ok(Ty::Json),
            "bytes" => Ok(Ty::Bytes),
            "int" => Ok(Ty::Int),
            _ => Err(()),
        }
    }
}

impl Ty {
    pub fn as_dr_type(&self) -> Cow<'static, str> {
        match self {
            Ty::Path => "path".into(),
            Ty::String => "string".into(),
            Ty::Json => "json".into(),
            Ty::Bytes => "bytes".into(),
            Ty::Int => "int".into(),
            Ty::ArrayString => "[string]".into(),
            Ty::ArrayBytes => "[bytes]".into(),
            Ty::MapPath => "{path}".into(),
            Ty::MapString => "{string}".into(),
            Ty::MapBytes => "{bytes}".into(),
            Ty::Struct(name) => Cow::Owned(name.to_string()),
            Ty::Union(types) => {
                let type_strs: Vec<_> = types.iter().map(|t| t.as_dr_type()).collect();
                Cow::Owned(type_strs.join(" | "))
            }
        }
    }
}

inventory::collect!(&'static CommandDef);
inventory::collect!(&'static StructDef);

static MODULES: Lazy<Vec<Module>> = Lazy::new(|| {
    let mut modules_map: HashMap<&str, Vec<&CommandDef>> = HashMap::new();

    // Group commands by module
    for command_def in inventory::iter::<&CommandDef>() {
        modules_map
            .entry(command_def.module)
            .or_insert_with(Vec::new)
            .push(command_def);
    }

    // Convert to Module structs
    let mut modules = Vec::new();
    for (module_name, command_defs) in modules_map {
        // Convert Vec<&CommandDef> to &'static [CommandDef]
        let commands: Vec<CommandDef> = command_defs.into_iter().cloned().collect();
        let commands_slice = Box::leak(commands.into_boxed_slice());

        modules.push(Module {
            name: module_name,
            commands: commands_slice,
        });
    }

    modules.sort_by(|a, b| a.name.cmp(b.name));
    modules
});

pub fn get_modules() -> &'static Vec<Module> {
    &*MODULES
}

pub fn get_structs() -> impl Iterator<Item = &'static StructDef> {
    inventory::iter::<&StructDef>().copied()
}

#[derive(Clone)]
pub struct Tap {
    pub key: Arc<str>,
    pub command: Arc<Command>,
    pub tap: Arc<dyn Fn(&str, &Command, &InputEvent) + Send + Sync>,
}

#[async_trait]
pub trait CommandRunner: Any
where
    Self: 'static,
{
    async fn forward(
        self: Arc<Self>,
        input: Input,
        _config: Arc<serde_json::Value>,
    ) -> Result<Input, Error> {
        Ok(input)
    }

    fn forward_stream(
        self: Arc<Self>,
        mut input: InputRx,
        output: InputTx,
        tap: Option<Tap>,
        config: Arc<serde_json::Value>,
    ) -> JoinHandle<Result<(), Error>>
    where
        Self: Send + Sync + 'static,
    {
        let this = self.clone();
        tokio::spawn(async move {
            loop {
                let event = input.recv().await.map_err(|e| Error(e.to_string()))?;
                let this = this.clone();
                match event {
                    InputEvent::Input(input) => {
                        let event = match this.forward(input, config.clone()).await {
                            Ok(output) => InputEvent::Input(output),
                            Err(e) => {
                                output
                                    .send(InputEvent::Error(e.clone()))
                                    .map_err(|e| Error(e.to_string()))?;
                                return Err(e);
                            }
                        };

                        if let Some(tap) = &tap {
                            (tap.tap)(&tap.key, &tap.command, &event);
                        }

                        output.send(event).map_err(|e| Error(e.to_string()))?;
                        output
                            .send(InputEvent::Finish)
                            .map_err(|e| Error(e.to_string()))?;
                    }
                    InputEvent::Finish => {
                        output
                            .send(InputEvent::Finish)
                            .map_err(|e| Error(e.to_string()))?;
                    }
                    InputEvent::Error(e) => {
                        output
                            .send(InputEvent::Error(e.clone()))
                            .map_err(|e| Error(e.to_string()))?;
                        return Err(e);
                    }
                    InputEvent::Close => {
                        output
                            .send(InputEvent::Close)
                            .map_err(|e| Error(e.to_string()))?;
                        break;
                    }
                }
            }
            Ok(())
        })
    }

    fn name(&self) -> &'static str;
}
