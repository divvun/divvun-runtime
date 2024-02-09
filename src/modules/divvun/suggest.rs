use std::{collections::HashMap, path::PathBuf, process::Stdio, sync::Arc};

use async_trait::async_trait;

use tokio::io::AsyncWriteExt;

use crate::ast;

use super::super::{CommandRunner, Context, Input, InputFut};

pub struct Suggest {
    _context: Arc<Context>,
    model_path: PathBuf,
    error_xml_path: PathBuf,
}

impl Suggest {
    pub fn new(
        context: Arc<Context>,
        mut kwargs: HashMap<String, ast::Arg>,
    ) -> Result<Arc<dyn CommandRunner>, anyhow::Error> {
        tracing::debug!("Creating suggest");
        let model_path = kwargs
            .remove("model_path")
            .and_then(|x| x.value)
            .ok_or_else(|| anyhow::anyhow!("model_path missing"))?;
        let error_xml_path = kwargs
            .remove("error_xml_path")
            .and_then(|x| x.value)
            .ok_or_else(|| anyhow::anyhow!("error_xml_path missing"))?;

        let model_path = context.extract_to_temp_dir(model_path)?;
        let error_xml_path = context.extract_to_temp_dir(error_xml_path)?;

        Ok(Arc::new(Self {
            _context: context,
            model_path,
            error_xml_path,
        }) as _)
    }
}

#[async_trait(?Send)]
impl CommandRunner for Suggest {
    async fn forward(self: Arc<Self>, input: InputFut) -> Result<Input, anyhow::Error> {
        let input = input.await?.try_into_string()?;

        let mut child = tokio::process::Command::new("divvun-suggest")
            // .arg("--json")
            .arg(&self.model_path)
            .arg(&self.error_xml_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .map_err(|e| {
                eprintln!("suggest ({}): {e:?}", self.model_path.display());
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
    fn name(&self) -> &'static str {
        "divvun::suggest"
    }
}
