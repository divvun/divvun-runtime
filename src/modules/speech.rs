use std::{collections::HashMap, path::PathBuf, sync::Arc, sync::Mutex};

use async_trait::async_trait;
use hfst::hfst_transducer::AnyTransducer;
use divvun_runtime_macros::{rt_command, rt_struct};
use divvun_speech::{Options, SAMPLE_RATE, Synthesizer};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use tokio::{fs::File, io::BufWriter};

use crate::{ast, modules::Error};

use super::{CommandRunner, Context, PipelineValue, PipelineValues};
use crate::modules::cg3::{self, Cohort, Reading};

/// Phonetic transcription using HFST
#[derive(facet::Facet)]
struct Phon {
    #[facet(opaque)]
    model: Mutex<AnyTransducer>,
    #[facet(opaque)]
    tag_models: IndexMap<String, Mutex<AnyTransducer>>,
}

#[rt_command(
    module = "speech",
    name = "phon",
    input = [String],
    output = "String",
    args = [model = "Path", tag_models = "MapPath"]
)]
impl Phon {
    pub async fn new(
        context: Arc<Context>,
        kwargs: HashMap<String, ast::Arg>,
    ) -> Result<Arc<dyn CommandRunner + Send + Sync>, super::Error> {
        let model_path = kwargs
            .get("model")
            .and_then(|x| x.value.as_ref())
            .and_then(|x| x.try_as_string())
            .ok_or_else(|| Error::msg("Missing model path").at("pipeline.json", "/args/model"))?;

        let tag_model_paths = kwargs
            .get("tag_models")
            .and_then(|x| x.value.as_ref())
            .and_then(|x| x.try_as_map_path())
            .ok_or_else(|| {
                Error::msg("Missing tag_models").at("pipeline.json", "/args/tag_models")
            })?;

        let model_path = context.extract_to_temp_dir(model_path).await?;
        let model = crate::modules::hfst::load_lookup(&model_path)?;
        let mut tag_models = IndexMap::new();
        for (k, v) in tag_model_paths.iter() {
            let path = context.extract_to_temp_dir(v).await?;
            tag_models.insert(k.clone(), crate::modules::hfst::load_lookup(&path)?);
        }

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

            let expansions = crate::modules::hfst::lookup_tags(model, phon, false);
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
        input: PipelineValue,
        _config: Arc<serde_json::Value>,
    ) -> Result<PipelineValues, crate::modules::Error> {
        let input = input.try_into_string()?;
        let output = self.process_cg3(&input);
        Ok(output.into())
    }

    fn name(&self) -> &'static str {
        "speech::phon"
    }
}

/// Text normalization using HFST transducers
#[derive(facet::Facet)]
struct Normalize {
    #[facet(opaque)]
    normalizers: IndexMap<String, Mutex<AnyTransducer>>,
    #[facet(opaque)]
    generator: Mutex<AnyTransducer>,
    #[facet(opaque)]
    analyzer: Mutex<AnyTransducer>,
}

#[derive(Debug, Clone)]
struct ReadingNode {
    reading_index: usize,
    depth: usize,
    subreadings: Vec<usize>,
}

#[derive(Debug, Clone)]
struct NormalizedReading {
    base_form: String,
    tags: Vec<String>,
    phonological_form: String,
    old_lemma: String,
    depth: usize, // Add depth for proper indentation
}

impl NormalizedReading {
    fn to_cg3_format(&self) -> String {
        let indent = "\t".repeat(self.depth);
        format!(
            "{}\"{}\" {} \"{}\"phon \"{}\"oldlemma",
            indent,
            self.base_form,
            self.tags.join(" "),
            self.phonological_form,
            self.old_lemma
        )
    }
}

#[derive(Debug, Clone)]
struct NormalizedCohort {
    readings: Vec<NormalizedReading>,
}

impl NormalizedCohort {
    fn to_cg3_format(&self) -> String {
        self.readings
            .iter()
            .map(|r| r.to_cg3_format())
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[rt_command(
    module = "speech",
    name = "normalize",
    input = [String],
    output = "String",
    args = [normalizers = "MapPath", generator = "Path", analyzer = "Path"]
)]
impl Normalize {
    pub async fn new(
        context: Arc<Context>,
        kwargs: HashMap<String, ast::Arg>,
    ) -> Result<Arc<dyn CommandRunner + Send + Sync>, super::Error> {
        // Load the HFST transducers from the context
        let normalizer_path_map = kwargs
            .get("normalizers")
            .and_then(|x| x.value.as_ref())
            .and_then(|x| x.try_as_map_path())
            .ok_or_else(|| {
                Error::msg("Missing normalizer paths").at("pipeline.json", "/args/normalizers")
            })?;

        let mut normalizer_paths = IndexMap::new();
        for (k, v) in normalizer_path_map.into_iter() {
            let path = context.extract_to_temp_dir(&v).await?;
            normalizer_paths.insert(k, path);
        }

        let generator_path = context
            .extract_to_temp_dir(
                &kwargs
                    .get("generator")
                    .and_then(|x| x.value.as_ref())
                    .and_then(|x| x.try_as_string())
                    .ok_or_else(|| {
                        Error::msg("Missing generator path").at("pipeline.json", "/args/generator")
                    })?,
            )
            .await?;

        let analyzer_path = context
            .extract_to_temp_dir(
                &kwargs
                    .get("analyzer")
                    .and_then(|x| x.value.as_ref())
                    .and_then(|x| x.try_as_string())
                    .ok_or_else(|| {
                        Error::msg("Missing analyzer path").at("pipeline.json", "/args/analyzer")
                    })?,
            )
            .await?;

        tracing::debug!("Loading normalizers");
        let mut normalizers = IndexMap::new();
        for (k, path) in normalizer_paths.into_iter() {
            tracing::debug!("adding HFST transducer for tag {}", k);
            normalizers.insert(k, crate::modules::hfst::load_lookup(&path)?);
        }
        tracing::debug!("Loading generator: {}", generator_path.display());
        let generator = crate::modules::hfst::load_lookup(&generator_path)?;
        tracing::debug!("Loading analyzer: {}", analyzer_path.display());
        let analyzer = crate::modules::hfst::load_lookup(&analyzer_path)?;

        Ok(Arc::new(Self {
            normalizers,
            generator,
            analyzer,
        }))
    }

    fn needs_expansion(&self, reading: &Reading) -> Option<&Mutex<AnyTransducer>> {
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
    ) -> Option<NormalizedReading> {
        let regen = format!("{}+{}", normalized_form, regentags);
        let regen_base_form = format!("{}+{}", reading.base_form.trim_matches('"'), regentags);

        tracing::debug!("2.b regenerating lookup: {}", regen);

        // Try regeneration with normalized form first
        let regenerations = crate::modules::hfst::lookup_tags(&self.generator, &regen, false);
        // Also try with base form as fallback
        let regenerations_base_form =
            crate::modules::hfst::lookup_tags(&self.generator, &regen_base_form, false);

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
            let reanalyses = crate::modules::hfst::lookup_tags(&self.analyzer, &phon, false);
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
                        return Some(NormalizedReading {
                            base_form: normalized_form.to_string(),
                            tags: regentags_spaced
                                .split_whitespace()
                                .map(|s| s.to_string())
                                .collect(),
                            phonological_form: phon.clone(),
                            old_lemma: reading.base_form.trim_matches('"').to_string(),
                            depth: reading.depth,
                        });
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

                let reanalyses = crate::modules::hfst::lookup_tags(&self.analyzer, &phon, false);
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
                            return Some(NormalizedReading {
                                base_form: normalized_form.to_string(),
                                tags: regentags_spaced
                                    .split_whitespace()
                                    .map(|s| s.to_string())
                                    .collect(),
                                phonological_form: phon.clone(),
                                old_lemma: reading.base_form.trim_matches('"').to_string(),
                                depth: reading.depth,
                            });
                        } else {
                            tracing::debug!("couldn't match {} and {}", reanal, regentags);
                        }
                    }
                }

                if reanalysis_failed {
                    tracing::debug!("3.b no analyses either...");
                    // Return fallback result when reanalysis fails completely
                    return Some(NormalizedReading {
                        base_form: normalized_form.to_string(),
                        tags: regentags
                            .replace("+", " ")
                            .split_whitespace()
                            .map(|s| s.to_string())
                            .collect(),
                        phonological_form: phon.clone(),
                        old_lemma: reading.base_form.trim_matches('"').to_string(),
                        depth: reading.depth,
                    });
                }
            }
        }

        None
    }

    fn process_expansion(
        &self,
        normalizer: &Mutex<AnyTransducer>,
        surface_form: &str,
        reading: &Reading,
    ) -> Option<NormalizedReading> {
        tracing::debug!(
            "1. looking up {} normaliser for {}",
            "[normalizer]",
            surface_form
        );

        let expansions = crate::modules::hfst::lookup_tags(normalizer, surface_form, false);

        tracing::debug!("Expansions: {:?}", expansions);

        let mut all_expansions = expansions;

        if all_expansions.is_empty() {
            tracing::debug!("Normaliser results empty.");
            // Try with extra full stop as in C++ version
            let expansions_dot =
                crate::modules::hfst::lookup_tags(normalizer, &format!("{surface_form}."), false);
            if !expansions_dot.is_empty() {
                tracing::debug!("Normalised with extra full stop!");
                all_expansions = expansions_dot;
            } else {
                return None;
            }
        }

        let regentags = self.extract_regentags(reading);

        for normalized_form in all_expansions.iter() {
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
        if let Some(normalized_form) = all_expansions.last() {
            tracing::debug!(
                "3.c All expansions failed, using last one: {}",
                normalized_form
            );

            return Some(NormalizedReading {
                base_form: normalized_form.clone(),
                tags: regentags
                    .replace("+", " ")
                    .split_whitespace()
                    .map(|s| s.to_string())
                    .collect(),
                phonological_form: normalized_form.clone(),
                old_lemma: reading.base_form.trim_matches('"').to_string(),
                depth: reading.depth,
            });
        }

        None
    }

    fn process_cohort(&self, cohort: &Cohort) -> Option<String> {
        tracing::debug!("Processing whole cohort");

        // Group readings by their hierarchical structure
        let mut reading_hierarchy = self.build_reading_hierarchy(&cohort.readings);

        // Process the hierarchy, building prefixes from subreadings
        self.process_reading_hierarchy(cohort, &mut reading_hierarchy)
    }

    fn build_reading_hierarchy(&self, readings: &[Reading]) -> Vec<ReadingNode> {
        let mut hierarchy: Vec<ReadingNode> = Vec::new();
        let mut stack: Vec<usize> = Vec::new();

        for (i, reading) in readings.iter().enumerate() {
            let node = ReadingNode {
                reading_index: i,
                depth: reading.depth,
                subreadings: Vec::new(),
            };

            // Pop from stack until we find a parent (or reach the root)
            while let Some(&parent_idx) = stack.last() {
                if hierarchy[parent_idx].depth < reading.depth {
                    break;
                }
                stack.pop();
            }

            // Add this node to the hierarchy
            let node_idx = hierarchy.len();
            hierarchy.push(node);

            // If we have a parent, add this node as its subreading
            if let Some(&parent_idx) = stack.last() {
                hierarchy[parent_idx].subreadings.push(node_idx);
            }

            stack.push(node_idx);
        }

        hierarchy
    }

    fn process_reading_hierarchy(
        &self,
        cohort: &Cohort,
        hierarchy: &mut Vec<ReadingNode>,
    ) -> Option<String> {
        // Find root readings (depth == 1)
        let root_indices: Vec<usize> = hierarchy
            .iter()
            .enumerate()
            .filter(|(_, node)| node.depth == 1)
            .map(|(i, _)| i)
            .collect();

        // Process each root reading with its subreadings
        for &root_idx in &root_indices {
            if let Some(result) = self.process_reading_node(cohort, hierarchy, root_idx) {
                return Some(result.to_cg3_format());
            }
        }

        tracing::debug!("No expansion tags in");
        None
    }

    fn process_reading_node(
        &self,
        cohort: &Cohort,
        hierarchy: &Vec<ReadingNode>,
        node_idx: usize,
    ) -> Option<NormalizedCohort> {
        let node = &hierarchy[node_idx];
        let reading = &cohort.readings[node.reading_index];

        tracing::debug!(
            "Processing reading node: base_form={}, depth={}, tags={:?}",
            reading.base_form,
            reading.depth,
            reading.tags
        );

        let mut all_readings = Vec::new();

        // Build prefix from all subreadings
        let mut prefix = String::new();
        for &subreading_idx in &node.subreadings {
            if let Some(subreading_prefix) =
                self.process_subreading_for_prefix(cohort, hierarchy, subreading_idx)
            {
                prefix.push_str(&subreading_prefix);
            }
        }

        // Collect all subreading normalized forms (they should get the same phonological form as main)
        for &subreading_idx in &node.subreadings {
            let subreading_node = &hierarchy[subreading_idx];
            let subreading_reading = &cohort.readings[subreading_node.reading_index];

            // Create a normalized reading for the subreading (will get updated phonological form later)
            let subreading_normalized = NormalizedReading {
                base_form: subreading_reading.base_form.trim_matches('"').to_string(),
                tags: self
                    .extract_regentags(subreading_reading)
                    .replace("+", " ")
                    .split_whitespace()
                    .map(|s| s.to_string())
                    .collect(),
                phonological_form: subreading_reading.base_form.trim_matches('"').to_string(), // Temporary
                old_lemma: subreading_reading.base_form.trim_matches('"').to_string(),
                depth: subreading_reading.depth,
            };
            all_readings.push(subreading_normalized);
        }

        // Check if this reading needs expansion due to normalizer tags
        let normalizer = self.needs_expansion(reading);
        let mut result = None;

        if let Some(normalizer) = normalizer {
            // Process with normalizer expansion
            let surface_form = self.extract_surface_form(cohort, reading);
            result = self.process_expansion(normalizer, &surface_form, reading);
        } else if !node.subreadings.is_empty() {
            // Process main reading when subreadings exist (expandmain logic)
            let surface_form = reading.base_form.trim_matches('"');
            let regentags = self.extract_regentags(reading);

            tracing::debug!("A. Regenerating from main lemma: {}", surface_form);
            result = self.try_regenerate_and_reanalyze(surface_form, &regentags, reading);
        }

        // Process the main reading
        if let Some(mut main) = result {
            // Combine prefix with result if we have both
            if !prefix.is_empty() {
                main = self.combine_prefix_with_main(&prefix, main);
                tracing::debug!("Combined prefix '{}' with main reading", prefix);

                // Also update all subreadings with the combined phonological form
                for subreading in &mut all_readings {
                    subreading.phonological_form = main.phonological_form.clone();
                }
            }

            // Add main reading at the beginning (it should come first)
            all_readings.insert(0, main);

            return Some(NormalizedCohort {
                readings: all_readings,
            });
        }

        None
    }

    fn process_subreading_for_prefix(
        &self,
        cohort: &Cohort,
        hierarchy: &Vec<ReadingNode>,
        node_idx: usize,
    ) -> Option<String> {
        let node = &hierarchy[node_idx];
        let reading = &cohort.readings[node.reading_index];

        tracing::debug!(
            "Processing subreading for prefix: base_form={}, depth={}, tags={:?}",
            reading.base_form,
            reading.depth,
            reading.tags
        );

        // Recursively build prefix from deeper subreadings first
        let mut prefix = String::new();
        for &deeper_subreading_idx in &node.subreadings {
            if let Some(deeper_prefix) =
                self.process_subreading_for_prefix(cohort, hierarchy, deeper_subreading_idx)
            {
                prefix.push_str(&deeper_prefix);
            }
        }

        // Process this subreading
        let normalizer = self.needs_expansion(reading);
        let surface_form = self.extract_surface_form(cohort, reading);

        let normalized_form = if let Some(normalizer) = normalizer {
            // Try to get normalized form from expansion
            if let Some(result) = self.process_expansion(normalizer, &surface_form, reading) {
                result.phonological_form
            } else {
                surface_form.to_string()
            }
        } else {
            surface_form.to_string()
        };

        prefix.push_str(&normalized_form);
        tracing::debug!("Built prefix from subreading: {}", prefix);
        Some(prefix)
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

    fn combine_prefix_with_main(
        &self,
        prefix: &str,
        reading: NormalizedReading,
    ) -> NormalizedReading {
        let combined_phon = format!("{}{}", prefix, reading.phonological_form);

        tracing::debug!(
            "Combining prefix '{}' with phon '{}' to create '{}'",
            prefix,
            reading.phonological_form,
            combined_phon
        );

        NormalizedReading {
            phonological_form: combined_phon,
            ..reading // Keep all other fields unchanged
        }
    }
}

#[async_trait]
impl CommandRunner for Normalize {
    async fn forward(
        self: Arc<Self>,
        input: PipelineValue,
        _config: Arc<serde_json::Value>,
    ) -> Result<PipelineValues, crate::modules::Error> {
        let input = input.try_into_string()?;

        // Parse the input using cg3::Output
        let output = self.process_cg3(&input);
        Ok(output.into())
    }

    fn name(&self) -> &'static str {
        "speech::normalize"
    }
}

/// Voice configuration for a single language
#[rt_struct(module = "speech")]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TtsVoiceConfig {
    pub name: String,
    pub language: usize,
    pub speakers: HashMap<u32, String>,
}

/// TTS configuration containing all available voices
#[rt_struct(module = "speech")]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TtsConfig {
    pub voices: HashMap<String, TtsVoiceConfig>,
}

/// Text-to-speech synthesis
#[derive(facet::Facet)]
struct Tts {
    speaker: i64,
    language: i64,
    #[facet(opaque)]
    speech: Synthesizer,
    #[facet(opaque)]
    config: Option<TtsConfig>,
}

#[rt_command(
    module = "speech",
    name = "tts",
    input = [String],
    output = "Bytes",
    kind = "audio",
    args = [voice_model = "Path", vocoder_model = "Path", speaker = "Int", language = "Int", config = "TtsConfig"]
)]
impl Tts {
    pub async fn new(
        context: Arc<Context>,
        kwargs: HashMap<String, ast::Arg>,
    ) -> Result<Arc<dyn CommandRunner + Send + Sync>, super::Error> {
        let voice_model = kwargs
            .get("voice_model")
            .and_then(|x| x.value.as_ref())
            .and_then(|x| x.try_as_string())
            .ok_or_else(|| {
                Error::msg("Missing voice_model").at("pipeline.json", "/args/voice_model")
            })?;
        let vocoder_model = kwargs
            .get("vocoder_model")
            .and_then(|x| x.value.as_ref())
            .and_then(|x| x.try_as_string())
            .ok_or_else(|| {
                Error::msg("Missing vocoder_model").at("pipeline.json", "/args/vocoder_model")
            })?;
        let speaker = kwargs
            .get("speaker")
            .and_then(|x| x.value.as_ref())
            .and_then(|x| x.try_as_int())
            .map(|x| x as i64)
            .ok_or_else(|| Error::msg("Missing speaker").at("pipeline.json", "/args/speaker"))?;
        let language = kwargs
            .get("language")
            .and_then(|x| x.value.as_ref())
            .and_then(|x| x.try_as_int())
            .map(|x| x as i64)
            .ok_or_else(|| Error::msg("Missing language").at("pipeline.json", "/args/language"))?;
        // let config = kwargs
        //     .get("config")
        //     .and_then(|x| x.value.as_ref())
        //     .map(|x| x.try_as_json())
        //     .ok_or_else(|| Error::msg("Missing config").at("pipeline.json", "/args/config"))?
        //     .map_err(|e| {
        //         Error::msg(format!("config is not valid JSON: {}", e))
        //             .at("pipeline.json", "/args/config")
        //     })?;
        // let config: TtsConfig = serde_json::from_value(config).map_err(|e| {
        //     Error::msg(format!("config is not valid TtsConfig: {}", e))
        //         .at("pipeline.json", "/args/config")
        // })?;

        let voice_model = context.extract_to_temp_dir(voice_model).await?;
        let vocoder_model = context.extract_to_temp_dir(vocoder_model).await?;

        let speech = Synthesizer::new(voice_model, vocoder_model).map_err(Error::wrap)?;

        Ok(Arc::new(Self {
            speaker,
            speech,
            language,
            config: None,
        }))
    }
}

fn generate_wav(samples: &[f32]) -> std::io::Result<Vec<u8>> {
    use std::io::Write;

    let sample_rate: u32 = SAMPLE_RATE;
    let num_channels: u16 = 1;
    let bits_per_sample: u16 = 32;
    let byte_rate = sample_rate * num_channels as u32 * bits_per_sample as u32 / 8;
    let block_align = num_channels * bits_per_sample / 8;
    let data_size = (samples.len() * 4) as u32;
    let file_size = 36 + data_size;

    let mut buf = Vec::with_capacity(44 + samples.len() * 4);

    // RIFF header
    buf.write_all(b"RIFF")?;
    buf.write_all(&file_size.to_le_bytes())?;
    buf.write_all(b"WAVE")?;

    // fmt chunk
    buf.write_all(b"fmt ")?;
    buf.write_all(&16u32.to_le_bytes())?; // chunk size
    buf.write_all(&3u16.to_le_bytes())?; // format = IEEE float
    buf.write_all(&num_channels.to_le_bytes())?;
    buf.write_all(&sample_rate.to_le_bytes())?;
    buf.write_all(&byte_rate.to_le_bytes())?;
    buf.write_all(&block_align.to_le_bytes())?;
    buf.write_all(&bits_per_sample.to_le_bytes())?;

    // data chunk
    buf.write_all(b"data")?;
    buf.write_all(&data_size.to_le_bytes())?;
    for sample in samples {
        buf.write_all(&sample.to_le_bytes())?;
    }

    Ok(buf)
}

/// SSML `<break>` sentinel format emitted by `cg3::sentences`: a single
/// "sentence" string of the shape `\x1FBREAK:NNNN\x1F` where NNNN is the
/// silence duration in milliseconds. Unit Separator (`0x1F`) is used as the
/// wrapper because it never appears in real text.
fn parse_break_sentinel(s: &str) -> Option<u32> {
    s.strip_prefix("\x1FBREAK:")?
        .strip_suffix('\x1F')?
        .parse::<u32>()
        .ok()
}

#[derive(Debug, Default, Clone, Copy)]
struct SentenceOpts {
    pace: Option<f32>,
}

/// Strip the `\x1FOPTS:k=v;k=v\x1F` prefix (if any) off a sentence and parse
/// it into structured overrides. Emitted by `cg3::sentences` when wrapping
/// SSML elements (e.g. `<prosody rate>`) tag the cohorts of a sentence.
fn parse_opts_prefix(s: &str) -> (SentenceOpts, &str) {
    let Some(rest) = s.strip_prefix("\x1FOPTS:") else {
        return (SentenceOpts::default(), s);
    };
    let Some(end) = rest.find('\x1F') else {
        return (SentenceOpts::default(), s);
    };
    let (kvs, after) = rest.split_at(end);
    let text = &after[1..]; // skip the closing \x1F
    let mut opts = SentenceOpts::default();
    for kv in kvs.split(';') {
        let Some((k, v)) = kv.split_once('=') else {
            continue;
        };
        match k {
            "pace" => opts.pace = v.parse::<f32>().ok(),
            _ => {}
        }
    }
    (opts, text)
}

fn silence_samples(ms: u32) -> Vec<f32> {
    let n = (SAMPLE_RATE as usize).saturating_mul(ms as usize) / 1000;
    vec![0.0_f32; n]
}

async fn speak_sentence(
    this: Arc<Tts>,
    sentence: String,
    speaker_id: i64,
    language_id: i64,
    pace: f32,
) -> Result<Vec<f32>, crate::modules::Error> {
    let samples = tokio::task::spawn_blocking(move || {
        let samples = this
            .speech
            .synthesize(
                &sentence,
                &Options {
                    pace,
                    speaker_id,
                    language_id,
                },
            )
            .map_err(Error::wrap)?;
        Ok(samples)
    })
    .await
    .map_err(Error::wrap)??;

    Ok(samples)
}

#[async_trait]
impl CommandRunner for Tts {
    async fn forward(
        self: Arc<Self>,
        input: PipelineValue,
        config: Arc<serde_json::Value>,
    ) -> Result<PipelineValues, crate::modules::Error> {
        let speaker = config
            .get("speaker")
            .and_then(|x| x.as_i64())
            .unwrap_or(self.speaker);
        let language = config
            .get("language")
            .and_then(|x| x.as_i64())
            .unwrap_or(self.language);
        let pace = config
            .get("pace")
            .and_then(|x| x.as_f64())
            .map(|x| x as f32)
            .unwrap_or(1.0);

        match input {
            PipelineValue::String(sentence) => {
                let samples = if let Some(ms) = parse_break_sentinel(&sentence) {
                    silence_samples(ms)
                } else {
                    let (opts, text) = parse_opts_prefix(&sentence);
                    let effective_pace = opts.pace.unwrap_or(pace);
                    speak_sentence(
                        self.clone(),
                        text.to_string(),
                        speaker,
                        language,
                        effective_pace,
                    )
                    .await?
                };
                let value = generate_wav(&samples).map_err(Error::wrap)?;

                Ok(value.into())
            }
            _ => Err(Error::msg("speech::tts expected a String input")),
        }
    }

    fn name(&self) -> &'static str {
        "speech::tts"
    }
}

#[cfg(test)]
mod tts_tests {
    use super::*;

    #[test]
    fn sentinel_round_trip() {
        assert_eq!(parse_break_sentinel("\x1FBREAK:500\x1F"), Some(500));
        assert_eq!(parse_break_sentinel("\x1FBREAK:0\x1F"), Some(0));
        assert_eq!(parse_break_sentinel("\x1FBREAK:1500\x1F"), Some(1500));
    }

    #[test]
    fn sentinel_rejects_non_matches() {
        assert_eq!(parse_break_sentinel("hello"), None);
        assert_eq!(parse_break_sentinel("BREAK:500"), None);
        assert_eq!(parse_break_sentinel("\x1FBREAK:abc\x1F"), None);
        assert_eq!(parse_break_sentinel("\x1FBREAK:500"), None);
        assert_eq!(parse_break_sentinel("BREAK:500\x1F"), None);
    }

    #[test]
    fn silence_sample_count() {
        // SAMPLE_RATE is a const u32 — half a second of silence is half that many samples.
        assert_eq!(silence_samples(500).len(), SAMPLE_RATE as usize / 2);
        assert_eq!(silence_samples(0).len(), 0);
        assert_eq!(silence_samples(1000).len(), SAMPLE_RATE as usize);
    }

    #[test]
    fn opts_prefix_extracted() {
        let (opts, text) = parse_opts_prefix("\x1FOPTS:pace=1.25\x1FHello world.");
        assert_eq!(opts.pace, Some(1.25));
        assert_eq!(text, "Hello world.");
    }

    #[test]
    fn opts_prefix_absent_passes_through() {
        let (opts, text) = parse_opts_prefix("Hello world.");
        assert!(opts.pace.is_none());
        assert_eq!(text, "Hello world.");
    }

    #[test]
    fn opts_prefix_unknown_keys_ignored() {
        let (opts, text) = parse_opts_prefix("\x1FOPTS:pace=2.0;unknown=foo\x1Fhi");
        assert_eq!(opts.pace, Some(2.0));
        assert_eq!(text, "hi");
    }

    #[test]
    fn opts_prefix_malformed_passes_through() {
        // Missing closing \x1F → treat as plain text.
        let (opts, text) = parse_opts_prefix("\x1FOPTS:pace=1.0 no closer");
        assert!(opts.pace.is_none());
        assert_eq!(text, "\x1FOPTS:pace=1.0 no closer");
    }
}
