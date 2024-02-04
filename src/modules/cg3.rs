use std::{
    future::Future,
    path::{Path, PathBuf},
    pin::Pin,
    process::Stdio,
    sync::Arc,
};

use tokio::io::AsyncWriteExt;

use crate::{
    ast,
    modules::{Arg, Command, Module, Ty},
};

use super::{Context, Input, InputFut};

inventory::submit! {
    Module {
        name: "cg3",
        commands: &[
            Command {
                name: "mwesplit",
                args: &[],
            },
            Command {
                name: "vislcg3",
                args: &[
                    Arg {
                        name: "model_path",
                        ty: Ty::Path,
                    },
                ],
            }
        ]
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
