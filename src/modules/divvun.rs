use std::{
    path::{Path, PathBuf},
    process::Stdio,
};

use tokio::io::AsyncWriteExt;

use super::InputFut;

pub async fn blanktag(model_path: PathBuf, input: InputFut<String>) -> anyhow::Result<String> {
    eprintln!("Running divvun::blanktag");
    let input = input.await?;

    let mut child = tokio::process::Command::new("divvun-blanktag")
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

pub async fn cgspell(
    err_model_path: PathBuf,
    acc_model_path: PathBuf,
    input: InputFut<String>,
) -> anyhow::Result<String> {
    eprintln!("Running divvun::cgspell");
    let input = input.await?;

    let mut child = tokio::process::Command::new("divvun-cgspell")
        .arg(err_model_path)
        .arg(acc_model_path)
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

pub async fn suggest(
    model_path: PathBuf,
    error_xml_path: PathBuf,
    input: InputFut<String>,
) -> anyhow::Result<String> {
    eprintln!("Running divvun::suggest");
    let input = input.await?;

    let mut child = tokio::process::Command::new("divvun-suggest")
        .arg("--json")
        .arg(model_path)
        .arg(error_xml_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()?;

    let mut stdin = child.stdin.take().unwrap();
    tokio::spawn(async move {
        stdin.write_all(input.as_bytes()).await.unwrap();
    });

    let output = child.wait_with_output().await?;

    let output = String::from_utf8(output.stdout)?;
    Ok(output)
}
