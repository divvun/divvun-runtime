use std::{future::Future, path::PathBuf, pin::Pin};

pub mod cg3;
pub mod divvun;
pub mod hfst;
pub mod speech;

pub type InputFut<T = String> = Pin<Box<dyn Future<Output = anyhow::Result<T>>>>;

pub struct Context {
    pub path: PathBuf,
}
