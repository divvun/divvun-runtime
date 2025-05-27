use std::{
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

use async_trait::async_trait;
use bitmask_enum::bitmask;
use box_format::{BoxFileReader, BoxPath, Compression};
use futures_util::Stream;
use memmap2::Mmap;
use tempfile::TempDir;
use tokio::{
    sync::broadcast::{Receiver, Sender},
    task::JoinHandle,
};

use crate::{
    ast::{self, PipelineDefinition},
    py,
    util::SharedBox,
};

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

#[derive(Clone, Debug, thiserror::Error)]
#[error("{0}")]
pub struct Error(pub(crate) String);

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
                tracing::debug!(
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
    pub commands: &'static [Command],
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
pub struct Command {
    pub name: &'static str,
    pub input: &'static [Ty],
    pub args: &'static [Arg],
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
    // pub optional: bool,
}

#[bitmask(u8)]
pub enum Ty {
    Path,
    String,
    Json,
    Bytes,
    Int,
    ArrayString,
    ArrayBytes,
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
    pub fn as_py_type(&self) -> String {
        let mut out = vec![];
        if self.contains(Ty::Path) {
            out.push("str");
        }
        if self.contains(Ty::String) {
            out.push("str");
        }
        if self.contains(Ty::Json) {
            out.push("Any");
        }
        if self.contains(Ty::Bytes) {
            out.push("bytes");
        }
        if self.contains(Ty::Int) {
            out.push("int");
        }
        if self.contains(Ty::ArrayString) {
            out.push("List[str]");
        }
        if self.contains(Ty::ArrayBytes) {
            out.push("List[bytes]");
        }
        out.join(" | ")
    }

    pub fn as_dr_type(&self) -> String {
        let mut out = vec![];
        if self.contains(Ty::Path) {
            out.push("path");
        }
        if self.contains(Ty::String) {
            out.push("string");
        }
        if self.contains(Ty::Json) {
            out.push("json");
        }
        if self.contains(Ty::Bytes) {
            out.push("bytes");
        }
        if self.contains(Ty::Int) {
            out.push("int");
        }
        if self.contains(Ty::ArrayString) {
            out.push("[string]");
        }
        if self.contains(Ty::ArrayBytes) {
            out.push("[bytes]");
        }
        out.join(" | ")
    }
}

inventory::collect!(Module);

#[async_trait]
pub trait CommandRunner
where
    Self: 'static,
{
    async fn forward(
        self: Arc<Self>,
        input: Input,
        config: Arc<serde_json::Value>,
    ) -> Result<Input, Error> {
        Ok(input)
    }

    fn forward_stream(
        self: Arc<Self>,
        mut input: InputRx,
        mut output: InputTx,
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
