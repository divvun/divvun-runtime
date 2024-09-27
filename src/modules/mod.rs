use std::{
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
use box_format::{BoxFileReader, BoxPath, Compression};
use memmap2::Mmap;
use tempfile::TempDir;

use crate::{ast::{self, PipelineDefinition}, util::SharedBox};

#[cfg(feature = "mod-cg3")]
pub mod cg3;
pub mod debug;
#[cfg(feature = "mod-divvun")]
pub mod divvun;
pub mod example;
#[cfg(feature = "mod-hfst")]
pub mod hfst;
pub mod runtime;
pub mod speech;
pub mod spell;

pub type InputFut = Pin<Box<dyn Future<Output = Result<Input, Error>> + Send>>;
pub type SharedInputFut = SharedBox<dyn Future<Output = Result<Input, Error>> + Send>;

#[derive(Debug, Clone)]
pub enum Input {
    Multiple(Box<[Input]>),
    String(String),
    Bytes(Vec<u8>),
    Json(serde_json::Value),
}

#[derive(Clone, Debug, thiserror::Error)]
#[error("{0}")]
pub struct Error(String);

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
                    let m = bf.read_bytes(record).map_err(|e| Error(e.to_string()))?.read_to_end(&mut buf);
                    serde_json::from_slice(&buf).map_err(|e| Error(e.to_string()))?
                };
                Ok(pipeline)
            },
            DataRef::Path(p) => {
                let p = p.join("pipeline.json");
                let f = std::fs::File::open(p).map_err(|e| Error(e.to_string()))?;
                let m = unsafe { Mmap::map(&f) }.map_err(|e| Error(e.to_string()))?;
                let pipeline: PipelineDefinition = serde_json::from_reader(&*m).map_err(|e| Error(e.to_string()))?;
                Ok(pipeline)
            },
        }
    }

    pub fn load_file(&self, path: impl AsRef<Path>) -> Result<impl Read, Error> {
        match &self.data {
            DataRef::BoxFile(bf, _) => {
                let record = bf
                    .find(&BoxPath::new(path).map_err(|e| Error(e.to_string()))?)
                    .map_err(|e| Error(e.to_string()))?
                    .as_file()
                    .unwrap();
                let out = bf.read_bytes(record).map_err(|e| Error(e.to_string()))?;
                Ok(out)
            }
            DataRef::Path(p) => {
                let out = std::fs::File::open(p.join("assets").join(path))
                    .map_err(|e| Error(e.to_string()))?;
                Ok(out.take(u64::MAX))
            }
        }
    }

    pub fn extract_to_temp_dir(&self, path: impl AsRef<Path>) -> Result<PathBuf, Error> {
        match &self.data {
            DataRef::BoxFile(bf, tmp) => {
                let bpath = BoxPath::new(path.as_ref()).map_err(|e| Error(e.to_string()))?;
                bf.extract_recursive(&bpath, tmp.path())
                    .map_err(|e| Error(e.to_string()))?;
                Ok(tmp.path().join(path.as_ref()))
            }
            DataRef::Path(p) => Ok(p.join("assets").join(path)),
        }
    }

    pub fn memory_map_file(&self, path: impl AsRef<Path>) -> Result<Mmap, Error> {
        match &self.data {
            DataRef::BoxFile(bf, _tmp) => {
                let bpath = BoxPath::new(path.as_ref()).map_err(|e| Error(e.to_string()))?;
                let node = bf.find(&bpath).map_err(|e| Error(e.to_string()))?;
                let node = node
                    .as_file()
                    .ok_or_else(|| Error("Not a file".to_string()))?;
                Ok(unsafe { bf.memory_map(node).map_err(|e| Error(e.to_string()))? })
            }
            DataRef::Path(p) => {
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

#[derive(Debug, Clone, Copy)]
pub struct Arg {
    pub name: &'static str,
    pub ty: Ty,
    // pub optional: bool,
}

#[derive(Debug, Clone, Copy)]
pub enum Ty {
    Path,
    String,
    Json,
    Bytes,
    Int,
}

impl FromStr for Ty {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
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
    pub fn as_rust_type(&self) -> &'static str {
        match self {
            Ty::Path => "PathBuf",
            Ty::String => "String",
            Ty::Json => "serde_json::Value",
            Ty::Bytes => "Vec<u8>",
            Ty::Int => "isize",
        }
    }

    pub fn as_py_type(&self) -> &'static str {
        match self {
            Ty::Path => "str",
            Ty::String => "str",
            Ty::Json => "Any",
            Ty::Bytes => "bytes",
            Ty::Int => "int",
        }
    }

    pub fn as_dr_type(&self) -> &'static str {
        match self {
            Ty::Path => "path",
            Ty::String => "string",
            Ty::Json => "json",
            Ty::Bytes => "bytes",
            Ty::Int => "int",
        }
    }
}

inventory::collect!(Module);

#[async_trait]
pub trait CommandRunner {
    async fn forward(
        self: Arc<Self>,
        input: SharedInputFut,
        config: Arc<serde_json::Value>,
    ) -> Result<Input, Error>;
    fn name(&self) -> &'static str;
}
