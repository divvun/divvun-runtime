use std::{
    collections::HashMap,
    fs::create_dir_all,
    sync::{Arc, OnceLock},
    thread::JoinHandle,
};

use async_trait::async_trait;
use divvun_speech::{Device, DivvunSpeech, Options, SymbolSet};
use indexmap::IndexMap;
use memmap2::Mmap;
use pathos::AppDirs;
use tokio::sync::{
    mpsc::{self, Receiver, Sender},
    Mutex,
};

use crate::{
    ast,
    modules::{Arg, CommandDef, Error, Module, Ty},
};

use super::{CommandRunner, Context, Input, SharedInputFut};
use cg3::{Cohort, Reading};

inventory::submit! {
    Module {
        name: "speech",
        commands: &[
            CommandDef {
                name: "tts",
                input: &[Ty::String],
                args: &[
                    Arg {
                        name: "voice_model",
                        ty: Ty::Path
                    },
                    Arg {
                        name: "univnet_model",
                        ty: Ty::Path
                    },
                    Arg {
                        name: "speaker",
                        ty: Ty::Int
                    },
                    Arg {
                        name: "language",
                        ty: Ty::Int,
                    },
                    Arg {
                        name: "alphabet",
                        ty: Ty::String,
                    }
                ],
                init: Tts::new,
                returns: Ty::Bytes,
            },
            CommandDef {
                name: "normalize",
                input: &[Ty::String],
                args: &[
                    Arg {
                        name: "normalizer",
                        ty: Ty::Path,
                    },
                    Arg {
                        name: "generator",
                        ty: Ty::Path,
                    },
                    Arg {
                        name: "sanalyzer",
                        ty: Ty::Path,
                    },
                    Arg {
                        name: "tags",
                        ty: Ty::ArrayString,
                    },
                ],
                init: Normalize::new,
                returns: Ty::String.or(Ty::ArrayString),
            },
            CommandDef {
                name: "phon",
                input: &[Ty::String],
                args: &[
                    Arg {
                        name: "model",
                        ty: Ty::Path,
                    },
                    Arg {
                        name: "tag_models",
                        ty: Ty::MapPath,
                    }
                ],
                init: Phon::new,
                returns: Ty::String,
            }
        ]
    }
}

struct Phon {
    model: hfst::Transducer,
    tag_models: IndexMap<String, hfst::Transducer>,
}

impl Phon {
    pub fn new(
        context: Arc<Context>,
        kwargs: HashMap<String, ast::Arg>,
    ) -> Result<Arc<dyn CommandRunner + Send + Sync>, super::Error> {
        let model_path = kwargs
            .get("model")
            .and_then(|x| x.value.as_ref())
            .and_then(|x| x.try_as_string())
            .ok_or_else(|| Error("Missing model path".to_string()))?;

        let tag_model_paths = kwargs
            .get("tag_models")
            .and_then(|x| x.value.as_ref())
            .and_then(|x| x.try_as_map_path())
            .ok_or_else(|| Error("Missing tag_models".to_string()))?;

        let model_path = context.extract_to_temp_dir(model_path)?;
        let model = hfst::Transducer::new(model_path);
        let tag_models = tag_model_paths
            .iter()
            .map(|(k, v)| {
                let path = context.extract_to_temp_dir(v)?;
                Ok((k.clone(), hfst::Transducer::new(path)))
            })
            .collect::<Result<IndexMap<_, _>, _>>()?;

        Ok(Arc::new(Self { model, tag_models }))
    }

    fn process_cohort(&self, cohort: &Cohort) -> Option<String> {
        for reading in &cohort.readings {
            let mut phon = cohort.word_form;
            tracing::debug!("Reading tags: {:?}", reading.tags);
            if let Some(cand) = reading.tags.iter().find(|tag| tag.ends_with("\"phon")) {
                tracing::debug!("Found phon tag: {}", cand);
                phon = &cand[1..cand.len() - 5];
            } else if let Some(cand) = reading
                .tags
                .iter()
                .find(|tag| tag.starts_with("\"<") && tag.ends_with(">\""))
            {
                tracing::debug!("Found re-analysed surface form: {}", cand);
                phon = &cand[2..cand.len() - 2];
            // } else if let Some(cand) = reading.tags.iter().find(|tag| tag.ends_with("\"MIDTAPE")) {
            //     tracing::debug!("Found MIDTAPE tag: {}", cand);
            //     phon = &cand[1..cand.len() - 8];
            } else {
                tracing::debug!("No phon tag found, using word form: {}", cohort.word_form);
            }

            let mut model = &self.model;
            for (tag, tag_model) in self.tag_models.iter() {
                if reading.tags.contains(&&**tag) {
                    tracing::debug!("Using tag model: {}", tag);
                    model = tag_model;
                    break;
                }
            }

            let expansions = model.lookup_tags(phon, false);
            if expansions.is_empty() {
                tracing::debug!("No expansions found");
                return None;
            }

            let mut new_output = reading
                .tags
                .iter()
                .filter(|tag| !tag.ends_with("\"phon"))
                .map(|tag| tag.to_string())
                .collect::<Vec<String>>();
            tracing::debug!("New output: {:?}", new_output);
            new_output.push(format!("\"{}\"phon", expansions.first().unwrap()));
            return Some(format!(
                "\t\"{}\" {}",
                reading.base_form,
                new_output.join(" ")
            ));
        }
        None
    }

    pub fn process_cg3(&self, text: &str) -> String {
        let output = cg3::Output::new(text);
        let mut result = String::new();

        for block in output.iter().filter_map(Result::ok) {
            match block {
                cg3::Block::Cohort(cohort) => {
                    if let Some(normalized) = self.process_cohort(&cohort) {
                        result.push_str("\"<");
                        result.push_str(&cohort.word_form);
                        result.push_str(">\"\n");
                        result.push_str(&normalized);
                        result.push('\n');
                    } else {
                        // If no normalization was applied, output the original cohort
                        result.push_str(&cohort.to_string());
                        result.push('\n');
                    }
                }
                cg3::Block::Text(text) => {
                    result.push_str(&text);
                    result.push('\n');
                }
                cg3::Block::Escaped(escaped) => {
                    result.push_str(":");
                    result.push_str(&escaped.to_string());
                    result.push('\n');
                }
            }
        }

        result
    }
}

#[async_trait]
impl CommandRunner for Phon {
    async fn forward(
        self: Arc<Self>,
        input: Input,
        config: Arc<serde_json::Value>,
    ) -> Result<Input, crate::modules::Error> {
        let input = input.try_into_string()?;
        let output = self.process_cg3(&input);
        Ok(output.into())
    }

    fn name(&self) -> &'static str {
        "speech::phon"
    }
}

struct Normalize {
    normalizer: hfst::Transducer,
    generator: hfst::Transducer,
    sanalyzer: hfst::Transducer,
    tags: Vec<String>,
}

impl Normalize {
    pub fn new(
        context: Arc<Context>,
        kwargs: HashMap<String, ast::Arg>,
    ) -> Result<Arc<dyn CommandRunner + Send + Sync>, super::Error> {
        // Load the HFST transducers from the context
        let normalizer_path = context.extract_to_temp_dir(
            &kwargs
                .get("normalizer")
                .and_then(|x| x.value.as_ref())
                .and_then(|x| x.try_as_string())
                .ok_or_else(|| Error("Missing normalizer path".to_string()))?,
        )?;

        let generator_path = context.extract_to_temp_dir(
            &kwargs
                .get("generator")
                .and_then(|x| x.value.as_ref())
                .and_then(|x| x.try_as_string())
                .ok_or_else(|| Error("Missing generator path".to_string()))?,
        )?;

        let sanalyzer_path = context.extract_to_temp_dir(
            &kwargs
                .get("sanalyzer")
                .and_then(|x| x.value.as_ref())
                .and_then(|x| x.try_as_string())
                .ok_or_else(|| Error("Missing sanalyzer path".to_string()))?,
        )?;

        let tags = kwargs
            .get("tags")
            .and_then(|x| x.value.as_ref())
            .and_then(|x| x.try_as_string_array())
            .ok_or_else(|| Error("Missing tags".to_string()))?;

        tracing::debug!("Loading normalizer: {}", normalizer_path.display());
        let normalizer = hfst::Transducer::new(normalizer_path);
        tracing::debug!("Loading generator: {}", generator_path.display());
        let generator = hfst::Transducer::new(generator_path);
        tracing::debug!("Loading sanalyzer: {}", sanalyzer_path.display());
        let sanalyzer = hfst::Transducer::new(sanalyzer_path);

        Ok(Arc::new(Self {
            normalizer,
            generator,
            sanalyzer,
            tags,
        }))
    }

    fn needs_expansion(&self, reading: &Reading) -> bool {
        if self.tags.is_empty() {
            return false;
        }
        for tag in self.tags.iter().map(|x| &**x) {
            if reading.tags.contains(&tag) {
                tracing::debug!("Expanding because of {}", tag);
                return true;
            }
        }
        false
    }

    fn extract_surface_form<'a>(&self, cohort: &'a Cohort, reading: &'a Reading) -> &'a str {
        // Try to find alternative surface form in tags
        for tag in &reading.tags {
            if tag.ends_with("\"phon") {
                tracing::debug!("Using Phon(?): {}", &tag[1..tag.len() - 5]);
                return &tag[1..tag.len() - 5];
            }
            if tag.starts_with("\"<") && tag.ends_with(">\"") {
                tracing::debug!("Using re-analysed surface form: {}", &tag[2..tag.len() - 2]);
                return &tag[2..tag.len() - 2];
            }
        }

        // Fall back to base form
        tracing::debug!("Using cohort word form: {}", cohort.word_form);
        cohort.word_form
    }

    fn extract_regentags(&self, reading: &Reading) -> String {
        let mut regentags = vec![];

        // Collect tags without slashes
        for tag in &reading.tags {
            if self.tags.iter().any(|x| tag == x) {
                continue;
            }

            if tag.chars().all(|c| c.is_ascii_alphanumeric()) {
                regentags.push(tag.to_string());
            }

            if tag.starts_with("#") {
                break;
            }
        }

        // Replace ++ with +
        for tag in regentags.iter_mut().filter(|x| x.contains("++")) {
            *tag = tag.replace("++", "+");
        }

        // Remove specific tags
        let removables = ["ABBR", "Cmpnd", "Err/Orth"];
        for r in removables {
            regentags.retain(|x| !x.contains(r));
        }

        // Remove expansion tags
        // for tag in &self.tags {
        //     let tag_with_plus = format!("+{}", tag);
        //     while let Some(p) = s.find(&tag_with_plus) {
        //         s.replace_range(p..p + tag_with_plus.len(), "");
        //     }
        // }

        // s
        regentags.join("+")
    }

    fn try_regenerate_and_reanalyze(
        &self,
        normalized_form: &str,
        regentags: &str,
        reading: &Reading,
    ) -> Option<String> {
        let regen = format!("{}+{}", normalized_form, regentags);
        let regen_base_form = format!("{}+{}", reading.base_form, regentags);

        tracing::debug!("2.b regenerating lookup: {}", regen);

        // Try regeneration
        let regenerations = self.generator.lookup_tags(&regen, false);
        let regenerations_base_form = self.generator.lookup_tags(&regen_base_form, false);

        let mut regenerated = false;
        let mut last_phon = None;

        tracing::debug!("regenerated: {:?}", regenerations);
        tracing::debug!("regenerated_base_form: {:?}", regenerations_base_form);

        for phon in regenerations {
            regenerated = true;
            last_phon = Some(phon.clone());

            tracing::debug!("3. reanalysing: {}", phon);

            // Try reanalysis
            let reanalyses = self.sanalyzer.lookup_tags(&phon, false);
            for reanal in reanalyses {
                if !reanal.contains("+Cmp") {
                    if !reanal.contains(regentags) {
                        tracing::debug!("couldn't match {} and {}", reanal, regentags);
                        tracing::trace!("NORMALISER_REMOVE:notagmatches1");
                        // return Some(format!(
                        //     ";{}\"{}\"{} \"{}\"phon {}oldlemma NORMALISER_REMOVE:notagmatches1",
                        //     regentags, normalized_form, reanal, phon, reading.base_form
                        // ));
                    } else {
                        return Some(format!(
                            "\t\"{}\" {} \"{}\"phon \"{}\"oldlemma",
                            normalized_form,
                            regentags.replace("+", " "),
                            normalized_form,
                            reading.base_form
                        ));
                    }
                }
            }
        }

        // If regeneration failed, try reanalyzing lemma
        if !regenerated {
            // if last_phon.is_none() {
            //     last_phon = Some(normalized_form.to_string());
            // }

            if let Some(phon) = last_phon {
                tracing::debug!("3. Couldn't regenerate, reanalysing lemma: {}", phon);

                let reanalyses = self.sanalyzer.lookup_tags(&phon, false);
                let mut reanalysis_failed = true;
                let mut last_reanal = None;

                for reanal in reanalyses.iter() {
                    reanalysis_failed = false;
                    tracing::debug!("3.a got: {}", reanal);

                    last_reanal = Some(reanal);

                    if !reanal.contains("+Cmp") {
                        if !reanal.contains(regentags) {
                            tracing::debug!("couldn't match {} and {}", reanal, regentags);
                            tracing::trace!("NORMALISER_REMOVE:notagmatches2");
                            return Some(format!(
                                ";{}\"{}\"{} \"{}\"phon {}oldlemma NORMALISER_REMOVE:notagmatches2",
                                regentags, normalized_form, reanal, phon, reading.base_form
                            ));
                        } else {
                            return Some(format!(
                                "{}\"{}\"{} \"{}\"phon {}oldlemma",
                                regentags, normalized_form, reanal, phon, reading.base_form
                            ));
                        }
                    }
                }

                if reanalysis_failed {
                    tracing::debug!("3.b no analyses either...");

                    return Some(format!(
                        "\t\"{}\" {} \"{}\"phon \"{}\"oldlemma",
                        normalized_form,
                        regentags.replace("+", " "),
                        last_reanal.as_deref().unwrap_or(&phon),
                        reading.base_form
                    ));
                }
            }
        }

        None
    }

    fn process_expansion(&self, surface_form: &str, reading: &Reading) -> Option<String> {
        tracing::debug!("Processing expansion for: {}", surface_form);
        let expansions = self.normalizer.lookup_tags(surface_form, false);

        tracing::debug!("Expansions: {:?}", expansions);
        if expansions.is_empty() {
            return None;
        }

        let regentags = self.extract_regentags(reading);

        for normalized_form in expansions.iter() {
            tracing::debug!("Trying normalized form: {}", normalized_form);

            tracing::debug!("Regentags: {}", regentags);
            if let Some(result) =
                self.try_regenerate_and_reanalyze(&normalized_form, &regentags, reading)
            {
                return Some(result);
            }
        }

        if let Some(normalized_form) = expansions.last() {
            tracing::debug!("3.c no analyses either...");

            return Some(format!(
                "\t\"{}\" {} \"{}\"phon \"{}\"oldlemma",
                normalized_form,
                regentags.replace("+", " "),
                normalized_form,
                reading.base_form
            ));
        }

        None
    }

    fn process_cohort(&self, cohort: &Cohort) -> Option<String> {
        for reading in &cohort.readings {
            if !self.needs_expansion(reading) {
                continue;
            }

            let surface_form = self.extract_surface_form(cohort, reading);
            if let Some(result) = self.process_expansion(&surface_form, reading) {
                return Some(result);
            }
        }
        None
    }

    fn process_cg3(&self, text: &str) -> String {
        let output = cg3::Output::new(text);
        let mut result = String::new();

        // Process each block
        for block in output.iter().filter_map(Result::ok) {
            match block {
                cg3::Block::Cohort(cohort) => {
                    if let Some(normalized) = self.process_cohort(&cohort) {
                        result.push_str("\"<");
                        result.push_str(&cohort.word_form);
                        result.push_str(">\"\n");
                        result.push_str(&normalized);
                        result.push('\n');
                    } else {
                        // If no normalization was applied, output the original cohort
                        result.push_str(&cohort.to_string());
                        result.push('\n');
                    }
                }
                cg3::Block::Text(text) => {
                    result.push_str(&text);
                    result.push('\n');
                }
                cg3::Block::Escaped(escaped) => {
                    result.push_str(":");
                    result.push_str(&escaped.to_string());
                    result.push('\n');
                }
            }
        }

        result
    }
}

#[async_trait]
impl CommandRunner for Normalize {
    async fn forward(
        self: Arc<Self>,
        input: Input,
        config: Arc<serde_json::Value>,
    ) -> Result<Input, crate::modules::Error> {
        let input = input.try_into_string()?;

        // Parse the input using cg3::Output
        let output = self.process_cg3(&input);
        Ok(output.into())
    }

    fn name(&self) -> &'static str {
        "speech::normalize"
    }
}

struct Tts {
    #[allow(unused)]
    voice_model: Mmap,
    #[allow(unused)]
    vocoder_model: Mmap,
    speaker: i32,
    language: i32,
    speech: DivvunSpeech<'static>,
}

impl Tts {
    pub fn new(
        context: Arc<Context>,
        kwargs: HashMap<String, ast::Arg>,
    ) -> Result<Arc<dyn CommandRunner + Send + Sync>, super::Error> {
        let voice_model = kwargs
            .get("voice_model")
            .and_then(|x| x.value.as_ref())
            .and_then(|x| x.try_as_string())
            .ok_or_else(|| Error("Missing voice_model".to_string()))?;
        let univnet_model = kwargs
            .get("univnet_model")
            .and_then(|x| x.value.as_ref())
            .and_then(|x| x.try_as_string())
            .ok_or_else(|| Error("Missing univnet_model".to_string()))?;
        let speaker = kwargs
            .get("speaker")
            .and_then(|x| x.value.as_ref())
            .and_then(|x| x.try_as_int())
            .map(|x| x as i32)
            .ok_or_else(|| Error("Missing speaker".to_string()))?;
        let language = kwargs
            .get("language")
            .and_then(|x| x.value.as_ref())
            .and_then(|x| x.try_as_int())
            .map(|x| x as i32)
            .ok_or_else(|| Error("Missing language".to_string()))?;
        let alphabet = kwargs
            .get("alphabet")
            .and_then(|x| x.value.as_ref())
            .and_then(|x| x.try_as_string())
            .ok_or_else(|| Error("Missing alphabet".to_string()))?;

        let voice_model = context.memory_map_file(voice_model)?;
        let vocoder_model = context.memory_map_file(univnet_model)?;

        let speech = unsafe {
            DivvunSpeech::from_memory_map(
                &voice_model,
                &vocoder_model,
                match &*alphabet {
                    "sme" => divvun_speech::SME_EXPANDED,
                    "smj" => divvun_speech::SMJ_EXPANDED,
                    "sma" => divvun_speech::SMA_EXPANDED,
                    "smi" => divvun_speech::ALL_SAMI,
                    other => return Err(Error(format!("Unknown alphabet: {other}"))),
                },
                Device::Cpu,
            )
        }
        .map_err(|e| Error(e.to_string()))?;

        Ok(Arc::new(Self {
            voice_model,
            vocoder_model,
            speaker,
            speech,
            language,
        }))
    }
}

async fn speak_sentence(
    this: Arc<Tts>,
    sentence: String,
    speaker: i32,
    language: i32,
) -> Result<Vec<u8>, crate::modules::Error> {
    let value = tokio::task::spawn_blocking(move || {
        let tensor = this
            .speech
            .forward(
                &sentence,
                Options {
                    pace: 1.05,
                    speaker,
                    language,
                },
            )
            .map_err(|e| Error(e.to_string()))?;

        DivvunSpeech::generate_wav(tensor).map_err(|e| Error(e.to_string()))
    })
    .await
    .map_err(|e| Error(e.to_string()))??;

    Ok(value)
}

#[async_trait]
impl CommandRunner for Tts {
    async fn forward(
        self: Arc<Self>,
        input: Input,
        config: Arc<serde_json::Value>,
    ) -> Result<Input, crate::modules::Error> {
        let speaker = config
            .get("speaker")
            .and_then(|x| x.as_i64())
            .map(|x| x as i32)
            .unwrap_or(self.speaker);
        let language = config
            .get("language")
            .and_then(|x| x.as_i64())
            .map(|x| x as i32)
            .unwrap_or(self.language);

        match input {
            Input::String(sentence) => {
                let value = speak_sentence(self.clone(), sentence, speaker, language).await?;
                Ok(value.into())
            }
            Input::ArrayString(sentences) => {
                let mut wavs = Vec::new();
                for sentence in sentences {
                    wavs.push(speak_sentence(self.clone(), sentence, speaker, language).await?);
                }

                let spec = hound::WavSpec {
                    channels: 1,
                    sample_rate: 22050,
                    bits_per_sample: 16,
                    sample_format: hound::SampleFormat::Int,
                };

                let out = Vec::with_capacity(wavs.iter().map(|x| x.len()).sum::<usize>() / 2 + 1);
                let mut out = std::io::Cursor::new(out);

                let mut writer = hound::WavWriter::new(&mut out, spec).unwrap();
                for data in wavs {
                    let mut reader = hound::WavReader::new(std::io::Cursor::new(data)).unwrap();

                    for sample in reader.into_samples::<i16>() {
                        let sample = sample.unwrap();
                        writer.write_sample(sample).unwrap();
                    }
                }

                drop(writer);

                Ok(out.into_inner().into())
            }
            _ => return Err(Error("Invalid input".to_string())),
        }
    }

    fn name(&self) -> &'static str {
        "speech::tts"
    }
}
