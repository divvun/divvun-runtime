use std::{
    path::{Path, PathBuf},
    process::Stdio,
    sync::Arc,
};

use tokio::io::AsyncWriteExt;

use super::{Context, Input, InputFut};

pub async fn tokenize(
    context: Arc<Context>,
    model_path: PathBuf,
    input: InputFut,
) -> anyhow::Result<Input> {
    // eprintln!("Running divvun::tokenize");
    let input = input.await?.try_into_string().unwrap();

    let mut child = tokio::process::Command::new("hfst-tokenize")
        .arg("-g")
        .arg(model_path)
        .current_dir(&context.path)
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
    Ok(output.into())
}
