use std::{collections::HashMap, process::Stdio, sync::Arc};

use async_trait::async_trait;
use tokio::io::AsyncWriteExt;

use crate::{
    ast,
    modules::{Arg, Command, Module, Ty},
};

use super::{CommandRunner, Context, Input, InputFut};

inventory::submit! {
    Module {
        name: "divvun",
        commands: &[
            Command {
                name: "blanktag",
                args: &[],
                init: Blanktag::new,
            },
            Command {
                name: "cgspell",
                args: &[
                    Arg {name: "err_model_path", ty: Ty::Path },
                    Arg {name: "acc_model_path", ty: Ty::Path },
                ],
                init: Cgspell::new,
            },
            Command {
                name: "suggest",
                args: &[
                    Arg {name: "model_path", ty: Ty::Path },
                    Arg {name: "error_xml_path", ty: Ty::Path },
                ],
                init: Suggest::new,
            }
        ]
    }
}

pub struct Blanktag {
    context: Arc<Context>,
    model_path: String,
}

impl Blanktag {
    pub fn new(
        context: Arc<Context>,
        mut kwargs: HashMap<String, ast::Arg>,
    ) -> Result<Arc<dyn CommandRunner>, anyhow::Error> {
        let model_path = kwargs
            .remove("model_path")
            .and_then(|x| x.value)
            .ok_or_else(|| anyhow::anyhow!("model_path missing"))?;

        Ok(Arc::new(Self {
            context,
            model_path,
        }) as _)
    }
}

#[async_trait(?Send)]
impl CommandRunner for Blanktag {
    async fn forward(self: Arc<Self>, input: InputFut) -> Result<Input, anyhow::Error> {
        let input = input.await?.try_into_string()?;

        let mut child = tokio::process::Command::new("divvun-blanktag")
            .arg(&self.model_path)
            .current_dir(&self.context.path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .map_err(|e| {
                eprintln!("divvun ({}): {e:?}", self.model_path);
                e
            })?;

        let mut stdin = child.stdin.take().unwrap();
        tokio::spawn(async move {
            stdin.write_all(input.as_bytes()).await.unwrap();
        });

        let output = child.wait_with_output().await?;
        if !output.status.success() {
            anyhow::bail!("Error")
        }

        let output = String::from_utf8(output.stdout)?;
        Ok(output.into())
    }
}

pub struct Cgspell {
    context: Arc<Context>,
    acc_model_path: String,
    err_model_path: String,
}

impl Cgspell {
    pub fn new(
        context: Arc<Context>,
        mut kwargs: HashMap<String, ast::Arg>,
    ) -> Result<Arc<dyn CommandRunner>, anyhow::Error> {
        let acc_model_path = kwargs
            .remove("acc_model_path")
            .and_then(|x| x.value)
            .ok_or_else(|| anyhow::anyhow!("acc_model_path missing"))?;
        let err_model_path = kwargs
            .remove("err_model_path")
            .and_then(|x| x.value)
            .ok_or_else(|| anyhow::anyhow!("err_model_path missing"))?;

        Ok(Arc::new(Self {
            context,
            acc_model_path,
            err_model_path,
        }) as _)
    }
}

#[async_trait(?Send)]
impl CommandRunner for Cgspell {
    async fn forward(self: Arc<Self>, input: InputFut) -> Result<Input, anyhow::Error> {
        let input = input.await?.try_into_string()?;

        let mut child = tokio::process::Command::new("divvun-cgspell")
            .arg(&self.err_model_path)
            .arg(&self.acc_model_path)
            .current_dir(&self.context.path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .map_err(|e| {
                eprintln!("divvun-cgspell ({}): {e:?}", self.acc_model_path);
                e
            })?;

        let mut stdin = child.stdin.take().unwrap();
        tokio::spawn(async move {
            stdin.write_all(input.as_bytes()).await.unwrap();
        });

        let output = child.wait_with_output().await?;
        if !output.status.success() {
            anyhow::bail!("Error")
        }

        let output = String::from_utf8(output.stdout)?;
        Ok(output.into())
    }
}

pub struct Suggest {
    context: Arc<Context>,
    model_path: String,
    error_xml_path: String,
}

impl Suggest {
    pub fn new(
        context: Arc<Context>,
        mut kwargs: HashMap<String, ast::Arg>,
    ) -> Result<Arc<dyn CommandRunner>, anyhow::Error> {
        let model_path = kwargs
            .remove("model_path")
            .and_then(|x| x.value)
            .ok_or_else(|| anyhow::anyhow!("model_path missing"))?;
        let error_xml_path = kwargs
            .remove("model_xml_path")
            .and_then(|x| x.value)
            .ok_or_else(|| anyhow::anyhow!("error_xml_path missing"))?;

        Ok(Arc::new(Self {
            context,
            model_path,
            error_xml_path,
        }) as _)
    }
}

#[async_trait(?Send)]
impl CommandRunner for Suggest {
    async fn forward(self: Arc<Self>, input: InputFut) -> Result<Input, anyhow::Error> {
        let input = input.await?.try_into_string()?;

        let mut child = tokio::process::Command::new("divvun-suggest")
            .arg("--json")
            .arg(&self.model_path)
            .arg(&self.error_xml_path)
            .current_dir(&self.context.path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .map_err(|e| {
                eprintln!("suggest ({}): {e:?}", self.model_path);
                e
            })?;

        let mut stdin = child.stdin.take().unwrap();
        tokio::spawn(async move {
            stdin.write_all(input.as_bytes()).await.unwrap();
        });

        let output = child.wait_with_output().await?;

        let output = String::from_utf8(output.stdout)?;
        Ok(output.into())
    }
}
