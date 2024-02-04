use std::{collections::HashMap, process::Stdio, sync::Arc};

use async_trait::async_trait;
use tokio::io::AsyncWriteExt;

use crate::{
    ast,
    modules::{cg3::Mwesplit, Arg, Command, Module, Ty},
};

use super::{CommandRunner, Context, Input, InputFut};

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

pub struct Tokenize {
    context: Arc<Context>,
    model_path: String,
}

impl Tokenize {
    pub fn new(
        context: Arc<Context>,
        mut kwargs: HashMap<String, ast::Arg>,
    ) -> Result<Arc<dyn CommandRunner>, anyhow::Error> {
        let model_path = kwargs
            .remove("model_path")
            .and_then(|x| x.value)
            .ok_or_else(|| anyhow::anyhow!("model_path missing"))?;

        Ok(Arc::new(Self {
            context,
            model_path,
        }) as _)
    }
}

#[async_trait(?Send)]
impl CommandRunner for Tokenize {
    async fn forward(self: Arc<Self>, input: InputFut) -> Result<Input, anyhow::Error> {
        let input = input.await?.try_into_string()?;

        let model_path = self.context.path.join(&self.model_path);
        let mut child = tokio::process::Command::new("hfst-tokenize")
            .arg("-g")
            .arg(model_path)
            .current_dir(&self.context.path)
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
}
