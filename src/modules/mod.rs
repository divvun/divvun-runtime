use std::{
    collections::HashMap,
    future::Future,
    io::Read,
    path::{Path, PathBuf},
    pin::Pin,
    str::FromStr,
    sync::Arc,
};

use async_trait::async_trait;
use box_format::{BoxFileReader, BoxPath};
use tempfile::TempDir;

use crate::{ast, util::SharedBox};

pub mod cg3;
pub mod divvun;
pub mod example;
pub mod hfst;
pub mod speech;
pub mod spell;

pub type InputFut = Pin<Box<dyn Future<Output = Result<Input, Arc<anyhow::Error>>>>>;
pub type SharedInputFut = SharedBox<dyn Future<Output = Result<Input, Arc<anyhow::Error>>>>;

#[derive(Debug, Clone)]
pub enum Input {
    Multiple(Box<[Input]>),
    String(String),
    Bytes(Vec<u8>),
    Json(serde_json::Value),
}

#[derive(Debug, thiserror::Error)]
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
    pub fn load_file(&self, path: impl AsRef<Path>) -> Result<impl Read, anyhow::Error> {
        match &self.data {
            DataRef::BoxFile(bf, _) => {
                let record = bf.find(&BoxPath::new(path)?)?.as_file().unwrap();
                let out = bf.read_bytes(record)?;
                Ok(out)
            }
            DataRef::Path(p) => {
                let out = std::fs::File::open(p.join("assets").join(path))?;
                Ok(out.take(u64::MAX))
            }
        }
    }

    pub fn extract_to_temp_dir(&self, path: impl AsRef<Path>) -> Result<PathBuf, anyhow::Error> {
        match &self.data {
            DataRef::BoxFile(bf, tmp) => {
                bf.extract_recursive(&BoxPath::new(path.as_ref())?, tmp.path())?;
                Ok(tmp.path().join(path.as_ref()))
            }
            DataRef::Path(p) => Ok(p.join("assets").join(path)),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Module {
    pub name: &'static str,
    pub commands: &'static [Command],
}

#[derive(Debug, Clone)]
pub struct Command {
    pub name: &'static str,
    pub input: &'static [Ty],
    pub args: &'static [Arg],
    pub init: fn(
        Arc<Context>,
        HashMap<String, ast::Arg>,
    ) -> Result<Arc<dyn CommandRunner>, anyhow::Error>,
    pub returns: Ty,
}

#[derive(Debug, Clone, Copy)]
pub struct Arg {
    pub name: &'static str,
    pub ty: Ty,
}

#[derive(Debug, Clone, Copy)]
pub enum Ty {
    Path,
    String,
    Json,
    Bytes,
}

impl FromStr for Ty {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "path" => Ok(Ty::Path),
            "string" => Ok(Ty::String),
            "json" => Ok(Ty::Json),
            "bytes" => Ok(Ty::Bytes),
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
        }
    }

    pub fn as_py_type(&self) -> &'static str {
        match self {
            Ty::Path => "str",
            Ty::String => "str",
            Ty::Json => "Any",
            Ty::Bytes => "bytes",
        }
    }

    pub fn as_dr_type(&self) -> &'static str {
        match self {
            Ty::Path => "path",
            Ty::String => "string",
            Ty::Json => "json",
            Ty::Bytes => "bytes",
        }
    }
}

inventory::collect!(Module);

#[async_trait(?Send)]
pub trait CommandRunner {
    async fn forward(self: Arc<Self>, input: SharedInputFut) -> Result<Input, Arc<anyhow::Error>>;
    fn name(&self) -> &'static str;
}
