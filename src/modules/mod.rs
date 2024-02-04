use std::{future::Future, path::PathBuf, pin::Pin};

pub mod cg3;
pub mod divvun;
pub mod example;
pub mod hfst;
pub mod speech;

pub type InputFut = Pin<Box<dyn Future<Output = anyhow::Result<Input>>>>;

#[derive(Debug, Clone)]
pub enum Input {
    String(String),
    Bytes(Vec<u8>),
}

impl Input {
    pub fn try_into_string(self) -> Option<String> {
        match self {
            Input::String(x) => Some(x),
            _ => None,
        }
    }

    pub fn try_into_bytes(self) -> Option<Vec<u8>> {
        match self {
            Input::Bytes(x) => Some(x),
            _ => None,
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

#[derive(Debug)]
pub struct Context {
    pub path: PathBuf,
}

#[derive(Debug, Clone, Copy)]
pub struct Module {
    pub name: &'static str,
    pub commands: &'static [Command],
}

#[derive(Debug, Clone, Copy)]
pub struct Command {
    pub name: &'static str,
    pub args: &'static [Arg],
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
}

impl Ty {
    pub fn as_rust_type(&self) -> &'static str {
        match self {
            Ty::Path => "PathBuf",
            Ty::String => "String",
        }
    }

    pub fn as_py_type(&self) -> &'static str {
        match self {
            Ty::Path => "str",
            Ty::String => "str",
        }
    }

    pub fn as_dr_type(&self) -> &'static str {
        match self {
            Ty::Path => "path",
            Ty::String => "string",
        }
    }
}

inventory::collect!(Module);
