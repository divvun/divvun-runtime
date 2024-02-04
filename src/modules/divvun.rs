use std::{path::PathBuf, process::Stdio, sync::Arc};

use tokio::io::AsyncWriteExt;

use crate::modules::{cg3::Mwesplit, Arg, Command, Module, Ty};

use super::{Context, Input, InputFut};

inventory::submit! {
    Module {
        name: "divvun",
        commands: &[
            Command {
                name: "blanktag",
                args: &[],
                init: Mwesplit::new,
            },
            Command {
                name: "cgspell",
                args: &[
                    Arg {name: "err_model_path", ty: Ty::Path },
                    Arg {name: "acc_model_path", ty: Ty::Path },
                ],
                init: Mwesplit::new,
            },
            Command {
                name: "suggest",
                args: &[
                    Arg {name: "model_path", ty: Ty::Path },
                    Arg {name: "error_xml_path", ty: Ty::Path },
                ],
                init: Mwesplit::new,
            }
        ]
    }
}

pub async fn blanktag(
    context: Arc<Context>,
    model_path: PathBuf,
    input: InputFut,
) -> anyhow::Result<Input> {
    // eprintln!("Running divvun::blanktag");
    let input = input.await?.try_into_string().unwrap();

    let mut child = tokio::process::Command::new("divvun-blanktag")
        .arg(&model_path)
        .current_dir(&context.path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|e| {
            eprintln!("divvun ({}): {e:?}", model_path.display());
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

pub async fn cgspell(
    context: Arc<Context>,
    err_model_path: PathBuf,
    acc_model_path: PathBuf,
    input: InputFut,
) -> anyhow::Result<Input> {
    // eprintln!("Running divvun::cgspell");
    let input = input.await?.try_into_string().unwrap();

    let mut child = tokio::process::Command::new("divvun-cgspell")
        .arg(&err_model_path)
        .arg(&acc_model_path)
        .current_dir(&context.path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|e| {
            eprintln!("divvun-cgspell ({}): {e:?}", acc_model_path.display());
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

pub async fn suggest(
    context: Arc<Context>,
    model_path: PathBuf,
    error_xml_path: PathBuf,
    input: InputFut,
) -> anyhow::Result<Input> {
    // eprintln!("Running divvun::suggest");
    let input = input.await?.try_into_string().unwrap();

    let mut child = tokio::process::Command::new("divvun-suggest")
        .arg("--json")
        .arg(&model_path)
        .arg(error_xml_path)
        .current_dir(&context.path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|e| {
            eprintln!("suggest ({}): {e:?}", model_path.display());
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
