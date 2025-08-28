use std::{collections::HashMap, path::PathBuf, sync::Arc};

use async_trait::async_trait;
use divvun_speech::{Device, DivvunSpeech, Options};
use indexmap::IndexMap;
use memmap2::Mmap;

use crate::{
    ast,
    modules::{Arg, CommandDef, Error, Module, Ty},
};

use super::{CommandRunner, Context, Input};
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
                        name: "normalizers",
                        ty: Ty::MapPath,
                    },
                    Arg {
                        name: "generator",
                        ty: Ty::Path,
                    },
                    Arg {
                        name: "analyzer",
                        ty: Ty::Path,
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
        _config: Arc<serde_json::Value>,
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
    normalizers: IndexMap<String, hfst::Transducer>,
    generator: hfst::Transducer,
    analyzer: hfst::Transducer,
}

impl Normalize {
    pub fn new(
        context: Arc<Context>,
        kwargs: HashMap<String, ast::Arg>,
    ) -> Result<Arc<dyn CommandRunner + Send + Sync>, super::Error> {
        // Load the HFST transducers from the context
        let normalizer_paths = kwargs
            .get("normalizers")
            .and_then(|x| x.value.as_ref())
            .and_then(|x| x.try_as_map_path())
            .ok_or_else(|| Error("Missing normalizer paths".to_string()))?
            .into_iter()
            .map(|(k, v)| (k, PathBuf::from(v)))
            .map(|(k, path)| context.extract_to_temp_dir(&path).map(|v| (k, v)))
            .collect::<Result<IndexMap<_, _>, _>>()?;

        let generator_path = context.extract_to_temp_dir(
            &kwargs
                .get("generator")
                .and_then(|x| x.value.as_ref())
                .and_then(|x| x.try_as_string())
                .ok_or_else(|| Error("Missing generator path".to_string()))?,
        )?;

        let analyzer_path = context.extract_to_temp_dir(
            &kwargs
                .get("analyzer")
                .and_then(|x| x.value.as_ref())
                .and_then(|x| x.try_as_string())
                .ok_or_else(|| Error("Missing analyzer path".to_string()))?,
        )?;

        tracing::debug!("Loading normalizers");
        let normalizers = normalizer_paths
            .into_iter()
            .map(|(k, path)| {
                tracing::debug!("adding HFST transducer for tag {}", k);
                (k, hfst::Transducer::new(&path))
            })
            .collect::<IndexMap<_, _>>();
        tracing::debug!("Loading generator: {}", generator_path.display());
        let generator = hfst::Transducer::new(generator_path);
        tracing::debug!("Loading analyzer: {}", analyzer_path.display());
        let analyzer = hfst::Transducer::new(analyzer_path);

        Ok(Arc::new(Self {
            normalizers,
            generator,
            analyzer,
        }))
    }

    fn needs_expansion(&self, reading: &Reading) -> Option<&hfst::Transducer> {
        if self.normalizers.is_empty() {
            return None;
        }

        self.normalizers.iter().find_map(|(tag, normalizer)| {
            if reading.tags.contains(&&**tag) {
                tracing::debug!("Expanding because of {}", tag);
                return Some(normalizer);
            }
            None
        })
    }

    fn extract_surface_form<'a>(&self, cohort: &'a Cohort, reading: &'a Reading) -> &'a str {
        // Try to find existing "phon tag first
        for tag in &reading.tags {
            if tag.ends_with("\"phon") {
                tracing::debug!("Using Phon(?): {}", &tag[1..tag.len() - 5]);
                return &tag[1..tag.len() - 5];
            }
        }

        // Then try alternative surface form "<>"
        for tag in &reading.tags {
            if tag.starts_with("\"<") && tag.ends_with(">\"") {
                tracing::debug!("Using re-analysed surface form: {}", &tag[2..tag.len() - 2]);
                return &tag[2..tag.len() - 2];
            }
        }

        // For subreadings (depth > 1), use the reading's base form
        if reading.depth > 1 {
            let result = reading.base_form.trim_matches('"');
            tracing::debug!("Using subreading base form: {}", result);
            return result;
        }

        // Fall back to lemma (removing quotes) for main readings
        let lemma = reading.base_form;
        if lemma.starts_with('"') && lemma.ends_with('"') {
            let result = &lemma[1..lemma.len() - 1];
            tracing::debug!("Using lemma: {}", result);
            result
        } else {
            tracing::debug!("Using cohort word form: {}", cohort.word_form);
            cohort.word_form
        }
    }

    fn extract_regentags(&self, reading: &Reading) -> String {
        let mut regentags = String::new();

        // Process tags to build regentags following C++ logic
        for tag in &reading.tags {
            // Stop at dependency markers
            if tag.starts_with("#") {
                break;
            }

            // Skip quoted tags, @ tags, bracketed tags, and tags with slashes
            if tag.starts_with('"') || tag.contains('@') || tag.contains('<') || tag.contains('/') {
                continue;
            }

            // Skip special markers
            if tag.contains("SELECT:")
                || tag.contains("MAP:")
                || tag.contains("SETPARENT:")
                || tag.contains("Cmp")
            {
                break;
            }

            // Add valid morphological tags
            if !regentags.is_empty() {
                regentags.push('+');
            }
            regentags.push_str(tag);
        }

        // Clean up the regentags string
        let mut s = regentags;

        // Replace ++ with +
        while let Some(pos) = s.find("++") {
            s.replace_range(pos..pos + 2, "+");
        }

        // Remove trailing +
        if s.ends_with('+') {
            s.pop();
        }

        // Remove specific problematic tags
        let removables = ["+ABBR", "+Cmpnd", "+Err/Orth"];
        for removable in removables {
            while let Some(pos) = s.find(removable) {
                s.replace_range(pos..pos + removable.len(), "");
            }
        }

        // Remove normalizer tags
        for normalizer_tag in self.normalizers.keys() {
            let tag_with_plus = format!("+{}", normalizer_tag);
            while let Some(pos) = s.find(&tag_with_plus) {
                s.replace_range(pos..pos + tag_with_plus.len(), "");
            }
        }

        s
    }

    fn try_regenerate_and_reanalyze(
        &self,
        normalized_form: &str,
        regentags: &str,
        reading: &Reading,
    ) -> Option<String> {
        let regen = format!("{}+{}", normalized_form, regentags);
        let regen_base_form = format!("{}+{}", reading.base_form.trim_matches('"'), regentags);

        tracing::debug!("2.b regenerating lookup: {}", regen);

        // Try regeneration with normalized form first
        let regenerations = self.generator.lookup_tags(&regen, false);
        // Also try with base form as fallback
        let regenerations_base_form = self.generator.lookup_tags(&regen_base_form, false);

        let mut regenerated = false;
        let mut last_phon = None;

        tracing::debug!("regenerated: {:?}", regenerations);
        tracing::debug!("regenerated_base_form: {:?}", regenerations_base_form);

        // Process regenerations from normalized form
        for phon in regenerations.iter().chain(regenerations_base_form.iter()) {
            regenerated = true;
            last_phon = Some(phon.clone());

            tracing::debug!("3. reanalysing: {}", phon);

            // Try reanalysis
            let reanalyses = self.analyzer.lookup_tags(&phon, false);
            for reanal in reanalyses {
                if !reanal.contains("+Cmp") {
                    // Extract tags part from reanalysis (everything after first +)
                    let reanal_tags = if let Some(pos) = reanal.find('+') {
                        reanal[pos..].to_string()
                    } else {
                        String::new()
                    };

                    // Convert both to space-separated for comparison
                    let reanal_spaced = reanal_tags.replace('+', " ");
                    let regentags_spaced = regentags.replace('+', " ");

                    // Check if regentags is contained in reanal_tags
                    if reanal_spaced.contains(&regentags_spaced) {
                        // Success case - tags match
                        // Format: new_lemma + tags + phon_tag + oldlemma_tag
                        return Some(format!(
                            "\t\"{}\" {} \"{}\"phon \"{}\"oldlemma",
                            normalized_form,
                            regentags_spaced,
                            phon,
                            reading.base_form.trim_matches('"')
                        ));
                    } else {
                        tracing::debug!("couldn't match {} and {}", reanal, regentags);
                        // Continue to next reanalysis instead of returning immediately
                    }
                }
            }
        }

        // If regeneration failed, try reanalyzing the normalized form directly
        if !regenerated {
            if last_phon.is_none() {
                last_phon = Some(normalized_form.to_string());
            }

            if let Some(phon) = last_phon {
                tracing::debug!("3. Couldn't regenerate, reanalysing lemma: {}", phon);

                let reanalyses = self.analyzer.lookup_tags(&phon, false);
                let reanalysis_failed = reanalyses.is_empty();

                for reanal in reanalyses.iter() {
                    tracing::debug!("3.a got: {}", reanal);

                    if !reanal.contains("+Cmp") {
                        let reanal_tags = if let Some(pos) = reanal.find('+') {
                            reanal[pos..].to_string()
                        } else {
                            String::new()
                        };

                        let reanal_spaced = reanal_tags.replace('+', " ");
                        let regentags_spaced = regentags.replace('+', " ");

                        if reanal_spaced.contains(&regentags_spaced) {
                            return Some(format!(
                                "\t\"{}\" {} \"{}\"phon \"{}\"oldlemma",
                                normalized_form,
                                regentags_spaced,
                                phon,
                                reading.base_form.trim_matches('"')
                            ));
                        } else {
                            tracing::debug!("couldn't match {} and {}", reanal, regentags);
                        }
                    }
                }

                if reanalysis_failed {
                    tracing::debug!("3.b no analyses either...");

                    // Don't return here - try next expansion first
                    // Only use this as final fallback if all expansions fail
                    // return Some(format!(
                    //     "\t\"{}\" {} \"{}\"phon \"{}\"oldlemma",
                    //     normalized_form,
                    //     regentags.replace("+", " "),
                    //     last_reanal.as_deref().unwrap_or(&phon),
                    //     reading.base_form.trim_matches('"')
                    // ));
                }
            }
        }

        None
    }

    fn process_expansion(
        &self,
        normalizer: &hfst::Transducer,
        surface_form: &str,
        reading: &Reading,
    ) -> Option<String> {
        tracing::debug!(
            "1. looking up {} normaliser for {}",
            "[normalizer]",
            surface_form
        );

        let expansions = normalizer.lookup_tags(surface_form, false);

        tracing::debug!("Expansions: {:?}", expansions);

        if expansions.is_empty() {
            tracing::debug!("Normaliser results empty.");
            // Try with extra full stop as in C++ version
            let expansions_dot = normalizer.lookup_tags(&format!("{surface_form}."), false);
            if !expansions_dot.is_empty() {
                tracing::debug!("Normalised with extra full stop!");
            }
            return None;
        }

        let regentags = self.extract_regentags(reading);

        for normalized_form in expansions.iter() {
            tracing::debug!("2.a Using normalised form: {}", normalized_form);

            tracing::debug!("Regentags: {}", regentags);
            if let Some(result) =
                self.try_regenerate_and_reanalyze(&normalized_form, &regentags, reading)
            {
                tracing::debug!(
                    "Expansion '{}' succeeded, returning result",
                    normalized_form
                );
                return Some(result);
            } else {
                tracing::debug!("Expansion '{}' failed, trying next", normalized_form);
            }
        }

        // Final fallback: if ALL expansions failed, use the last one anyway
        if let Some(normalized_form) = expansions.last() {
            tracing::debug!(
                "3.c All expansions failed, using last one: {}",
                normalized_form
            );

            return Some(format!(
                "\t\"{}\" {} \"{}\"phon \"{}\"oldlemma",
                normalized_form,
                regentags.replace("+", " "),
                normalized_form,
                reading.base_form.trim_matches('"')
            ));
        }

        None
    }

    fn process_cohort(&self, cohort: &Cohort) -> Option<String> {
        tracing::debug!("Processing whole cohort");

        let mut expand_main = false;

        // Check if any reading has subreadings (this would trigger expandmain logic)
        for reading in &cohort.readings {
            // Check for compound/subreading markers
            if reading.tags.iter().any(|tag| {
                tag.contains("Cmp")
                    || tag.contains("compound")
                    || tag.contains("<cohort-with-dynamic-compound>")
            }) {
                expand_main = true;
                break;
            }
        }

        // Process subreadings first to build prefix
        let mut prefix = String::new();
        let mut main_result = None;

        for reading in &cohort.readings {
            tracing::debug!(
                "Processing reading: base_form={}, depth={}, tags={:?}",
                reading.base_form,
                reading.depth,
                reading.tags
            );

            // Check if this reading needs expansion due to normalizer tags
            let normalizer = self.needs_expansion(reading);

            if let Some(normalizer) = normalizer {
                // Process with normalizer expansion
                let surface_form = self.extract_surface_form(cohort, reading);
                if let Some(result) = self.process_expansion(normalizer, &surface_form, reading) {
                    if reading.depth > 1 {
                        // This is a subreading - extract the normalized form for prefix
                        if let Some(normalized_form) =
                            self.extract_normalized_form_from_result(&result)
                        {
                            prefix.push_str(&normalized_form);
                            tracing::debug!("Adding subreading to prefix: {}", normalized_form);
                        }
                    } else {
                        // Main reading with normalization
                        main_result = Some(result);
                    }
                }
            } else if expand_main && reading.depth == 1 {
                // Process main reading when subreadings exist (expandmain logic)
                let surface_form = reading.base_form.trim_matches('"');
                let regentags = self.extract_regentags(reading);

                tracing::debug!("A. Regenerating from main lemma: {}", surface_form);

                if let Some(result) =
                    self.try_regenerate_and_reanalyze(surface_form, &regentags, reading)
                {
                    main_result = Some(result);
                }
            }
        }

        // Combine prefix with main result if we have both
        if !prefix.is_empty() && main_result.is_some() {
            let main = main_result.as_ref().unwrap();
            if let Some(combined) = self.combine_prefix_with_main(&prefix, main) {
                tracing::debug!("Combined prefix '{}' with main reading", prefix);
                return Some(combined);
            }
        }

        // Return main result if no prefix combination needed
        if let Some(result) = main_result {
            return Some(result);
        }

        tracing::debug!("No expansion tags in");

        None
    }

    fn process_cg3(&self, text: &str) -> String {
        let output = cg3::Output::new(text);
        let mut result = String::new();
        let mut everything_has_failed = true;

        // Process each block
        for block in output.iter().filter_map(Result::ok) {
            match block {
                cg3::Block::Cohort(cohort) => {
                    if let Some(normalized) = self.process_cohort(&cohort) {
                        everything_has_failed = false;
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
                    result.push(':');
                    result.push_str(&escaped.to_string());
                    result.push('\n');
                }
            }
        }

        if everything_has_failed {
            tracing::debug!("no usable results, printing source");
        }

        result
    }

    fn extract_normalized_form_from_result(&self, result: &str) -> Option<String> {
        // Extract the normalized form from a result string like:
        // "\t\"seedee\" v2 N \"seedee\"phon \"CD\"oldlemma"
        // We want to extract "seedee"
        if let Some(start) = result.find("\t\"") {
            let after_tab_quote = start + 2;
            if let Some(end) = result[after_tab_quote..].find('"') {
                let normalized_form = &result[after_tab_quote..after_tab_quote + end];
                return Some(normalized_form.to_string());
            }
        }
        None
    }

    fn combine_prefix_with_main(&self, prefix: &str, main_result: &str) -> Option<String> {
        // Combine prefix with main result by replacing the main lemma with prefix+lemma
        // Input: prefix="seedee", main_result="\t\"spiller\" N Sg Nom \"spiller\"phon \"spiller\"oldlemma"
        // Output: "\t\"seedeespiller\" N Sg Nom \"seedeespiller\"phon \"spiller\"oldlemma"

        if let Some(start) = main_result.find("\t\"") {
            let after_tab_quote = start + 2;
            if let Some(end) = main_result[after_tab_quote..].find('"') {
                let main_lemma = &main_result[after_tab_quote..after_tab_quote + end];
                let combined_lemma = format!("{}{}", prefix, main_lemma);

                // Replace both the main lemma and the phon tag
                let mut result = main_result.to_string();
                result = result.replace(
                    &format!("\"{}\"", main_lemma),
                    &format!("\"{}\"", combined_lemma),
                );

                // Also update the phon tag if it exists
                if result.contains(&format!("\"{}\"phon", main_lemma)) {
                    result = result.replace(
                        &format!("\"{}\"phon", main_lemma),
                        &format!("\"{}\"phon", combined_lemma),
                    );
                }

                return Some(result);
            }
        }

        None
    }
}

#[async_trait]
impl CommandRunner for Normalize {
    async fn forward(
        self: Arc<Self>,
        input: Input,
        _config: Arc<serde_json::Value>,
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
                    let reader = hound::WavReader::new(std::io::Cursor::new(data)).unwrap();

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
