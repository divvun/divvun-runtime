use std::{collections::HashMap, path::PathBuf, process::Stdio, sync::Arc};

use async_trait::async_trait;
use tokio::io::AsyncWriteExt;

use crate::{
    ast,
    modules::{Arg, Command, Module, Ty},
};

use super::{CommandRunner, Context, Input, InputFut};

inventory::submit! {
    Module {
        name: "cg3",
        commands: &[
            Command {
                name: "mwesplit",
                args: &[],
                init: Mwesplit::new,
            },
            Command {
                name: "vislcg3",
                args: &[
                    Arg {
                        name: "model_path",
                        ty: Ty::Path,
                    },
                ],
                init: Mwesplit::new,
            }
        ]
    }
}

pub struct Mwesplit {
    context: Arc<Context>,
}

impl Mwesplit {
    pub fn new(
        context: Arc<Context>,
        _kwargs: HashMap<String, ast::Arg>,
    ) -> Result<Arc<dyn CommandRunner>, anyhow::Error> {
        Ok(Arc::new(Mwesplit { context }) as _)
    }
}

#[async_trait(?Send)]
impl CommandRunner for Mwesplit {
    async fn forward(self: Arc<Self>, input: InputFut) -> Result<Input, anyhow::Error> {
        let input = input.await?.try_into_string().unwrap();

        let mut child = tokio::process::Command::new("cg-mwesplit")
            .current_dir(&self.context.path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .map_err(|e| {
                eprintln!("mwesplit: {e:?}");
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

pub async fn mwesplit(context: Arc<Context>, input: InputFut) -> anyhow::Result<Input> {
    let input = input.await?.try_into_string().unwrap();

    let mut child = tokio::process::Command::new("cg-mwesplit")
        .current_dir(&context.path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|e| {
            eprintln!("mwesplit: {e:?}");
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

pub async fn vislcg3(
    context: Arc<Context>,
    model_path: PathBuf,
    input: InputFut,
) -> anyhow::Result<Input> {
    let input = input.await?.try_into_string().unwrap();

    let mut child = tokio::process::Command::new("vislcg3")
        .arg("-g")
        .arg(&model_path)
        .current_dir(&context.path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|e| {
            eprintln!("vislcg3 ({}): {e:?}", model_path.display());
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
