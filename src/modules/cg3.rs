use std::{path::Path, process::Stdio};

use tokio::io::AsyncWriteExt;

pub async fn mwesplit(input: &str) -> anyhow::Result<String> {
    let mut child = tokio::process::Command::new("cg-mwesplit")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()?;

    let mut stdin = child.stdin.take().unwrap();
    stdin.write_all(input.as_bytes()).await?;

    let output = child.wait_with_output().await?;

    let output = String::from_utf8(output.stdout)?;
    Ok(output)
}

pub async fn vislcg3(model_path: &Path, input: &str) -> anyhow::Result<String> {
    let mut child = tokio::process::Command::new("vislcg3")
        .arg("-g")
        .arg(model_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()?;

    let mut stdin = child.stdin.take().unwrap();
    stdin.write_all(input.as_bytes()).await?;

    let output = child.wait_with_output().await?;

    let output = String::from_utf8(output.stdout)?;
    Ok(output)
}
