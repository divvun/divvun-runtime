use std::{
    path::{Path, PathBuf},
    process::Stdio,
};

use tokio::io::AsyncWriteExt;

use super::InputFut;

pub async fn tokenize(model_path: PathBuf, input: InputFut<String>) -> anyhow::Result<String> {
    // eprintln!("Running divvun::tokenize");
    let input = input.await?;

    let mut child = tokio::process::Command::new("hfst-tokenize")
        .arg("-g")
        .arg(model_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()?;

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
