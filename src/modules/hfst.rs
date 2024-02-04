use std::{path::PathBuf, process::Stdio, sync::Arc};

use tokio::io::AsyncWriteExt;

use crate::modules::{cg3::Mwesplit, Arg, Command, Module, Ty};

use super::{Context, Input, InputFut};

inventory::submit! {
    Module {
        name: "hfst",
        commands: &[
            Command {
                name: "tokenize",
                args: &[Arg { name: "model_path", ty: Ty::Path }],
                init: Mwesplit::new,
            }
        ]
    }
}

pub async fn tokenize(
    context: Arc<Context>,
    model_path: PathBuf,
    input: InputFut,
) -> anyhow::Result<Input> {
    tracing::info!("Running tokenize");
    let input = input.await?.try_into_string().unwrap();

    let model_path = context.path.join(model_path);
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
