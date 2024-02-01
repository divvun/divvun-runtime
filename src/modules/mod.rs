use std::{future::Future, path::PathBuf, pin::Pin};

pub mod cg3;
pub mod divvun;
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
