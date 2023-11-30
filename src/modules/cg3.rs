use std::{
    future::Future,
    path::{Path, PathBuf},
    pin::Pin,
    process::Stdio,
    sync::Arc,
};

use tokio::io::AsyncWriteExt;

use super::{Context, InputFut};

pub async fn mwesplit(context: Arc<Context>, input: InputFut<String>) -> anyhow::Result<String> {
    let input = input.await?;

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
    Ok(output)
}

pub async fn vislcg3(
    context: Arc<Context>,
    model_path: PathBuf,
    input: InputFut<String>,
) -> anyhow::Result<String> {
    let input = input.await?;

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
    Ok(output)
}
