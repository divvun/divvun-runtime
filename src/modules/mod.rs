use std::{future::Future, pin::Pin};

pub mod cg3;
pub mod divvun;
pub mod hfst;

pub type InputFut<T = String> = Pin<Box<dyn Future<Output = anyhow::Result<T>>>>;
