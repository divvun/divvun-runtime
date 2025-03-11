use std::{
    collections::HashMap,
    fs::create_dir_all,
    sync::{Arc, OnceLock},
    thread::JoinHandle,
};

use async_trait::async_trait;
use divvun_speech::{Device, DivvunSpeech, Options, SymbolSet};
use memmap2::Mmap;
use pathos::AppDirs;
use tokio::sync::{
    mpsc::{self, Receiver, Sender},
    Mutex,
};

use crate::{
    ast,
    modules::{Arg, Command, Error, Module, Ty},
};

use super::{CommandRunner, Context, Input, SharedInputFut};
use cg3::{Cohort, Reading};

inventory::submit! {
    Module {
        name: "speech",
        commands: &[
            Command {
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
                        name: "alphabet",
                        ty: Ty::String,
                    }
                ],
                init: Tts::new,
                returns: Ty::Bytes,
            },
            Command {
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
            }
        ]
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

        println!("Loading normalizer: {}", normalizer_path.display());
        let normalizer = hfst::Transducer::new(normalizer_path);
        println!("Loading generator: {}", generator_path.display());
        let generator = hfst::Transducer::new(generator_path);
        println!("Loading sanalyzer: {}", sanalyzer_path.display());
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

    fn extract_surface_form<'a>(&self, reading: &'a Reading) -> &'a str {
        // Try to find alternative surface form in tags
        for tag in &reading.tags {
            if tag.starts_with("\"<") && tag.ends_with(">\"") {
                tracing::debug!("Using re-analysed surface form: {}", &tag[2..tag.len() - 2]);
                return &tag[2..tag.len() - 2];
            }
            if tag.ends_with("\"phon") {
                tracing::debug!("Using Phon(?): {}", &tag[1..tag.len() - 6]);
                return &tag[1..tag.len() - 6];
            }
        }

        // Fall back to base form
        tracing::debug!("Using base form: {}", reading.base_form);
        reading.base_form
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

        tracing::debug!("2.b regenerating lookup: {}", regen);

        // Try regeneration
        let regenerations = self.generator.lookup_tags(&regen, false);
        let mut regenerated = false;
        let mut last_phon = None;

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
                    if let Some(reanal) = last_reanal {
                        return Some(format!(
                            "{}\"{}\"{} \"{}\"phon {}oldlemma",
                            regentags, normalized_form, reanal, phon, reading.base_form
                        ));
                    }
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

        for normalized_form in expansions {
            tracing::debug!("Trying normalized form: {}", normalized_form);
            let regentags = self.extract_regentags(reading);
            tracing::debug!("Regentags: {}", regentags);
            if let Some(result) =
                self.try_regenerate_and_reanalyze(&normalized_form, &regentags, reading)
            {
                return Some(result);
            }
        }

        None
    }

    fn process_cohort(&self, cohort: &Cohort) -> Option<String> {
        for reading in &cohort.readings {
            if !self.needs_expansion(reading) {
                continue;
            }

            let surface_form = self.extract_surface_form(reading);
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
        input: SharedInputFut,
        config: Arc<serde_json::Value>,
    ) -> Result<Input, crate::modules::Error> {
        let input = input.await?.try_into_string()?;

        // Parse the input using cg3::Output
        let output = self.process_cg3(&input);
        let output = cg3::Output::new(&output);
        // let result = output.to_string();
        let mut result = String::new();

        // Process each block
        for block in output.iter().filter_map(Result::ok) {
            match block {
                cg3::Block::Cohort(cohort) => {
                    if let Some(reading) = cohort.readings.first() {
                        result.push_str(&reading.base_form);
                    }
                }
                cg3::Block::Text(text) => {}
                cg3::Block::Escaped(escaped) => {
                    result.push_str(&escaped);
                }
            }
        }

        Ok(result.into())
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
        }))
    }
}

async fn speak_sentence(
    this: Arc<Tts>,
    sentence: String,
    speaker: i32,
) -> Result<Vec<u8>, crate::modules::Error> {
    let value = tokio::task::spawn_blocking(move || {
        let tensor = this
            .speech
            .forward(
                &sentence,
                Options {
                    pace: 1.05,
                    speaker,
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
        input: SharedInputFut,
        config: Arc<serde_json::Value>,
    ) -> Result<Input, crate::modules::Error> {
        let speaker = config
            .get("speaker")
            .and_then(|x| x.as_i64())
            .map(|x| x as i32)
            .unwrap_or(self.speaker);

        match input.await? {
            Input::String(sentence) => {
                let value = speak_sentence(self.clone(), sentence, speaker).await?;
                Ok(value.into())
            }
            Input::ArrayString(sentences) => {
                let mut wavs = Vec::new();
                for sentence in sentences {
                    wavs.push(speak_sentence(self.clone(), sentence, speaker).await?);
                }
                Ok(Input::Multiple(
                    wavs.into_iter()
                        .map(Input::Bytes)
                        .collect::<Vec<_>>()
                        .into(),
                ))
            }
            _ => return Err(Error("Invalid input".to_string())),
        }
    }

    fn name(&self) -> &'static str {
        "speech::tts"
    }
}
