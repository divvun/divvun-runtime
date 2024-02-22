use std::{collections::HashMap, path::PathBuf, process::Stdio, sync::Arc};

use async_trait::async_trait;

use cg3::Block;
use serde::Serialize;
use tokio::io::AsyncWriteExt;

use crate::{ast, modules::SharedInputFut};

use super::super::{CommandRunner, Context, Input};

// pub struct Suggest {
//     _context: Arc<Context>,
//     model_path: PathBuf,
//     error_xml_path: PathBuf,
// }

// impl Suggest {
//     pub fn new(
//         context: Arc<Context>,
//         mut kwargs: HashMap<String, ast::Arg>,
//     ) -> Result<Arc<dyn CommandRunner>, anyhow::Error> {
//         tracing::debug!("Creating suggest");
//         let model_path = kwargs
//             .remove("model_path")
//             .and_then(|x| x.value)
//             .ok_or_else(|| anyhow::anyhow!("model_path missing"))?;
//         let error_xml_path = kwargs
//             .remove("error_xml_path")
//             .and_then(|x| x.value)
//             .ok_or_else(|| anyhow::anyhow!("error_xml_path missing"))?;

//         let model_path = context.extract_to_temp_dir(model_path)?;
//         let error_xml_path = context.extract_to_temp_dir(error_xml_path)?;

//         Ok(Arc::new(Self {
//             _context: context,
//             model_path,
//             error_xml_path,
//         }) as _)
//     }
// }

// #[async_trait(?Send)]
// impl CommandRunner for Suggest {
//     async fn forward(self: Arc<Self>, input: SharedInputFut) -> Result<Input, Arc<anyhow::Error>> {
//         let input = input
//             .await?
//             .try_into_string()
//             .map_err(|e| Arc::new(e.into()))?;

//         let mut child = tokio::process::Command::new("divvun-suggest")
//             .arg("--json")
//             .arg(&self.model_path)
//             .arg(&self.error_xml_path)
//             .stdin(Stdio::piped())
//             .stdout(Stdio::piped())
//             .spawn()
//             .map_err(|e| {
//                 eprintln!("suggest ({}): {e:?}", self.model_path.display());
//                 e
//             })
//             .map_err(|e| Arc::new(e.into()))?;

//         let mut stdin = child.stdin.take().unwrap();
//         tokio::spawn(async move {
//             stdin.write_all(input.as_bytes()).await.unwrap();
//         });

//         let output = child
//             .wait_with_output()
//             .await
//             .map_err(|e| Arc::new(e.into()))?;

//         let output: serde_json::Value =
//             serde_json::from_slice(output.stdout.as_slice()).map_err(|e| Arc::new(e.into()))?;
//         Ok(output.into())
//     }

//     fn name(&self) -> &'static str {
//         "divvun::suggest"
//     }
// }
pub struct Suggest {
    _context: Arc<Context>,
    generator: hfst::Transducer,
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

        let generator = hfst::Transducer::new(model_path);

        Ok(Arc::new(Self {
            _context: context,
            generator,
            error_xml_path,
        }) as _)
    }
}

#[derive(Debug, Serialize)]
struct SuggestResult<'a> {
    word: &'a str,
    char_offset: usize,
    utf16_offset: usize,
    byte_offset: usize,
}

#[derive(Debug, Serialize)]
struct SuggestOutput<'a> {
    results: Vec<SuggestResult<'a>>,
    input: String,
}

#[async_trait(?Send)]
impl CommandRunner for Suggest {
    async fn forward(self: Arc<Self>, input: SharedInputFut) -> Result<Input, Arc<anyhow::Error>> {
        let input = input
            .await?
            .try_into_string()
            .map_err(|e| Arc::new(e.into()))?;
        let input = cg3::Output::new(&input);

        let original_input = input
            .iter()
            .filter_map(|x| match x {
                Ok(Block::Cohort(x)) => Some(Ok(x.word_form)),
                Ok(Block::Escaped(x)) => Some(Ok(x)),
                Ok(Block::Text(_)) => None,
                Err(e) => Some(Err(e)),
            })
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| Arc::new(e.into()))?;

        let mut char_offset = 0usize;
        let mut byte_offset = 0usize;
        let mut utf16_offset = 0usize;

        let results = input
            .iter()
            .filter_map(|x| match x {
                Ok(Block::Cohort(x)) => {
                    // proc_reading(self.)
                    // x.readings.iter().map(|x| {
                    //     let tags = self.generator.lookup_tags(x.raw_line));
                    // });
                    // // let sforms = );

                    let out = SuggestResult {
                        word: x.word_form,
                        char_offset,
                        byte_offset,
                        utf16_offset,
                    };

                    char_offset += x.word_form.chars().count();
                    byte_offset += x.word_form.as_bytes().len();
                    utf16_offset += x.word_form.encode_utf16().count();
                    Some(Ok(out))
                }
                Ok(Block::Escaped(x)) => {
                    char_offset += x.chars().count();
                    byte_offset += x.as_bytes().len();
                    utf16_offset += x.encode_utf16().count();
                    None
                }
                Ok(Block::Text(_)) => None,
                Err(e) => Some(Err(e)),
            })
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| Arc::new(e.into()))?;

        let output = serde_json::to_string(&SuggestOutput {
            input: original_input.join(""),
            results,
        })
        .unwrap();
        Ok(output.into())
    }
    fn name(&self) -> &'static str {
        "divvun::suggest"
    }
}
