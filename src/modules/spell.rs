use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use divvun_fst::{speller::Speller, transducer::thfst::MmapThfstTransducer};
use divvun_runtime_macros::rt_command;

use crate::ast;
use crate::util::worker::Worker;

use super::{CommandRunner, Context, Error, PipelineValue, PipelineValues};

/// Spelling suggestion using divvun_fst
#[derive(facet::Facet)]
struct Suggest {
    #[facet(opaque)]
    _context: Arc<Context>,
    #[facet(opaque)]
    worker: Worker<String, String>,
}

#[rt_command(
    module = "spell",
    name = "suggest",
    input = [String],
    output = "Json",
    args = [lexicon_path = "Path", mutator_path = "Path"]
)]
impl Suggest {
    pub async fn new(
        context: Arc<Context>,
        mut kwargs: HashMap<String, ast::Arg>,
    ) -> Result<Arc<dyn CommandRunner + Send + Sync>, Error> {
        use divvun_fst::tokenizer::Tokenize as _;

        let lexicon_path = kwargs
            .remove("lexicon_path")
            .and_then(|x| x.value)
            .and_then(|x| x.try_as_string())
            .ok_or_else(|| {
                Error::msg("lexicon_path missing").at("pipeline.json", "/args/lexicon_path")
            })?;
        let mutator_path = kwargs
            .remove("mutator_path")
            .and_then(|x| x.value)
            .and_then(|x| x.try_as_string())
            .ok_or_else(|| {
                Error::msg("mutator_path missing").at("pipeline.json", "/args/mutator_path")
            })?;

        let model_context = context.clone();
        let worker = Worker::spawn(move || {
            let lexicon = model_context
                .load_fst::<MmapThfstTransducer>(&lexicon_path)
                .unwrap();
            let mutator = model_context
                .load_fst::<MmapThfstTransducer>(&mutator_path)
                .unwrap();
            let speller = divvun_fst::speller::HfstSpeller::new(mutator, lexicon);

            move |input: String| {
                let results = input
                    .word_bound_indices()
                    .map(|(pos, word)| {
                        let results = speller.clone().suggest(&word);
                        serde_json::json!({ "index": pos, "word": word, "suggestions": results })
                    })
                    .collect::<Vec<_>>();

                serde_json::to_string(&results).unwrap()
            }
        });

        Ok(Arc::new(Self {
            _context: context,
            worker,
        }) as _)
    }
}

#[async_trait]
impl CommandRunner for Suggest {
    async fn forward(
        self: Arc<Self>,
        input: PipelineValue,
        _config: Arc<serde_json::Value>,
    ) -> Result<PipelineValues, crate::modules::Error> {
        let input = input.try_into_string()?;

        let value = self
            .worker
            .call(input)
            .await
            .map_err(|e| Error::msg(format!("spell::suggest: {e}")))?;

        Ok(value.into())
    }

    fn name(&self) -> &'static str {
        "spell::suggest"
    }
}
