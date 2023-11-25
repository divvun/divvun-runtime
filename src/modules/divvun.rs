use std::{path::Path, process::Stdio};

use tokio::io::AsyncWriteExt;

pub async fn blanktag(model_path: &Path, input: &str) -> anyhow::Result<String> {
    let mut child = tokio::process::Command::new("divvun-blanktag")
        .arg(model_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()?;

    let mut stdin = child.stdin.take().unwrap();
    stdin.write_all(input.as_bytes()).await?;

    let output = child.wait_with_output().await?;
    if !output.status.success() {
        anyhow::bail!("Error")
    }

    let output = String::from_utf8(output.stdout)?;
    Ok(output)
}

pub async fn cgspell(
    err_model_path: &Path,
    acc_model_path: &Path,
    input: &str,
) -> anyhow::Result<String> {
    let mut child = tokio::process::Command::new("divvun-cgspell")
        .arg(err_model_path)
        .arg(acc_model_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()?;

    let mut stdin = child.stdin.take().unwrap();
    stdin.write_all(input.as_bytes()).await?;

    let output = child.wait_with_output().await?;
    if !output.status.success() {
        anyhow::bail!("Error")
    }

    let output = String::from_utf8(output.stdout)?;
    Ok(output)
}

pub async fn suggest(
    model_path: &Path,
    error_xml_path: &Path,
    input: &str,
) -> anyhow::Result<String> {
    let mut child = tokio::process::Command::new("divvun-suggest")
        .arg("--json")
        .arg(model_path)
        .arg(error_xml_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()?;

    let mut stdin = child.stdin.take().unwrap();
    stdin.write_all(input.as_bytes()).await?;

    let output = child.wait_with_output().await?;

    let output = String::from_utf8(output.stdout)?;
    Ok(output)
}
