use std::{
    any::Any,
    borrow::Cow,
    collections::HashMap,
    fmt::{Display, Write},
    future::Future,
    path::{Path, PathBuf},
    pin::Pin,
    str::FromStr,
    sync::Arc,
};

use once_cell::sync::Lazy;

use async_trait::async_trait;
use box_format::{BoxFileReader, BoxPath, Compression};
use mmap_io::{MemoryMappedFile, segment::Segment};
use tokio::{
    io::AsyncReadExt,
    sync::broadcast::{Receiver, Sender},
    task::JoinHandle,
};

use crate::{
    ast::{self, Command, PipelineBundle, PipelineDefinition},
    util::SharedBox,
};

// Simple glob matching for patterns like "errors-*.ftl"
fn glob_match(pattern: &str, text: &str) -> bool {
    // Simple implementation for patterns like "errors-*.ftl"
    if let Some(star_pos) = pattern.find('*') {
        let prefix = &pattern[..star_pos];
        let suffix = &pattern[star_pos + 1..];
        text.starts_with(prefix) && text.ends_with(suffix)
    } else {
        pattern == text
    }
}

pub mod debug;
pub mod example;
pub mod runtime;
pub mod spell;

#[cfg(feature = "mod-cg3")]
pub mod cg3;

#[cfg(feature = "mod-cg3")]
pub(crate) mod cg3_util;

#[cfg(feature = "mod-divvun")]
pub mod divvun;

#[cfg(feature = "mod-hfst")]
pub mod hfst;

#[cfg(feature = "mod-jq")]
pub mod jq;

#[cfg(feature = "mod-speech")]
pub mod speech;

#[cfg(feature = "mod-ssml")]
pub mod ssml;

pub type PipelineValueFut = Pin<Box<dyn Future<Output = Result<PipelineValue, Error>> + Send>>;
pub type SharedPipelineValueFut =
    SharedBox<dyn Future<Output = Result<PipelineValue, Error>> + Send>;

#[derive(Debug, Clone)]
pub enum PipelineEvent {
    Value(PipelineValue),
    Error(Error),
    Finish,
    /// "Stop the work you're doing for this forward() call." Discard any in-flight
    /// emission, forward downstream, then wait for the next value. The pipeline
    /// stays alive — distinct from Close, which tears it down.
    Cancel,
    Close,
}

pub type PipelineValueTx = Sender<PipelineEvent>;
pub type PipelineValueRx = Receiver<PipelineEvent>;

/// Owned interleaved floating-point audio produced by a pipeline stage.
#[derive(Debug, Clone)]
pub struct AudioBuffer {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
    pub channels: u16,
    /// Optional word ranges expressed as sample indices into `samples`.
    pub word_timings: Vec<AudioWordTiming>,
}

/// A word and its half-open sample range in an [`AudioBuffer`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioWordTiming {
    pub word: String,
    pub start_sample: usize,
    pub end_sample: usize,
}

impl AudioBuffer {
    pub fn to_wav_bytes(&self) -> std::io::Result<Vec<u8>> {
        use std::io::{Error as IoError, ErrorKind, Write};

        if self.sample_rate == 0 {
            return Err(IoError::new(
                ErrorKind::InvalidInput,
                "audio sample rate must be non-zero",
            ));
        }
        if self.channels == 0 {
            return Err(IoError::new(
                ErrorKind::InvalidInput,
                "audio channel count must be non-zero",
            ));
        }
        if self.samples.len() % self.channels as usize != 0 {
            return Err(IoError::new(
                ErrorKind::InvalidInput,
                "audio samples must contain complete channel frames",
            ));
        }

        let data_size = self
            .samples
            .len()
            .checked_mul(std::mem::size_of::<f32>())
            .and_then(|size| u32::try_from(size).ok())
            .ok_or_else(|| {
                IoError::new(ErrorKind::InvalidInput, "audio data exceeds WAV limits")
            })?;
        let block_align = self
            .channels
            .checked_mul(std::mem::size_of::<f32>() as u16)
            .ok_or_else(|| IoError::new(ErrorKind::InvalidInput, "invalid WAV block alignment"))?;
        let byte_rate = self
            .sample_rate
            .checked_mul(block_align as u32)
            .ok_or_else(|| IoError::new(ErrorKind::InvalidInput, "invalid WAV byte rate"))?;
        let file_size = 36_u32.checked_add(data_size).ok_or_else(|| {
            IoError::new(ErrorKind::InvalidInput, "audio data exceeds WAV limits")
        })?;
        let mut output = Vec::with_capacity(44 + data_size as usize);

        output.write_all(b"RIFF")?;
        output.write_all(&file_size.to_le_bytes())?;
        output.write_all(b"WAVE")?;
        output.write_all(b"fmt ")?;
        output.write_all(&16_u32.to_le_bytes())?;
        output.write_all(&3_u16.to_le_bytes())?;
        output.write_all(&self.channels.to_le_bytes())?;
        output.write_all(&self.sample_rate.to_le_bytes())?;
        output.write_all(&byte_rate.to_le_bytes())?;
        output.write_all(&block_align.to_le_bytes())?;
        output.write_all(&32_u16.to_le_bytes())?;
        output.write_all(b"data")?;
        output.write_all(&data_size.to_le_bytes())?;
        for sample in &self.samples {
            output.write_all(&sample.to_le_bytes())?;
        }

        Ok(output)
    }
}

/// A single value flowing through a pipeline. Multiplicity is expressed via
/// `PipelineValues` at the return-type level (see `CommandRunner::forward`),
/// not via dedicated array variants.
#[derive(Debug, Clone)]
pub enum PipelineValue {
    String(String),
    Bytes(Vec<u8>),
    Json(serde_json::Value),
    Audio(AudioBuffer),
}

/// Ordered sequence of values produced by a single `forward()` call. A length-1
/// `PipelineValues` is the common case (single in, single out); longer sequences
/// express batch producers (e.g. sentence splitting).
#[derive(Debug, Clone, Default)]
pub struct PipelineValues(pub Vec<PipelineValue>);

impl IntoIterator for PipelineValues {
    type Item = PipelineValue;
    type IntoIter = std::vec::IntoIter<PipelineValue>;
    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl Display for PipelineEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PipelineEvent::Value(x) => write!(f, "{:#}", x)?,
            PipelineEvent::Error(x) => write!(f, "Error: {:#}", x)?,
            PipelineEvent::Finish => write!(f, "Finish")?,
            PipelineEvent::Cancel => write!(f, "Cancel")?,
            PipelineEvent::Close => write!(f, "Close")?,
        }
        Ok(())
    }
}

impl Display for PipelineValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if f.alternate() {
            match self {
                PipelineValue::String(x) => write!(f, "{}", x),
                PipelineValue::Bytes(x) => write!(f, "<<{} bytes>>", x.len()),
                PipelineValue::Json(x) => {
                    write!(f, "{}", serde_json::to_string_pretty(&x).unwrap())
                }
                PipelineValue::Audio(x) => write!(
                    f,
                    "<<{} audio samples, {} Hz, {} channel(s)>>",
                    x.samples.len(),
                    x.sample_rate,
                    x.channels
                ),
            }
        } else {
            match self {
                PipelineValue::String(x) => write!(f, "{}", x),
                PipelineValue::Bytes(x) => write!(f, "<<{} bytes>>", x.len()),
                PipelineValue::Json(x) => write!(f, "{}", serde_json::to_string(&x).unwrap()),
                PipelineValue::Audio(x) => write!(
                    f,
                    "<<{} audio samples, {} Hz, {} channel(s)>>",
                    x.samples.len(),
                    x.sample_rate,
                    x.channels
                ),
            }
        }
    }
}

/// Error location as file + path (not byte offsets)
#[derive(Clone, Debug, Default)]
pub struct ErrorLocation {
    /// File name (e.g., "pipeline.json", "config.json")
    pub file: String,
    /// JSON path (e.g., "/commands/tok/args/model_path")
    pub path: String,
}

impl std::fmt::Display for ErrorLocation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.file.is_empty() && self.path.is_empty() {
            Ok(())
        } else if self.path.is_empty() {
            write!(f, " at {}", self.file)
        } else {
            write!(f, " at {}:{}", self.file, self.path)
        }
    }
}

/// The inner error content
#[derive(Clone, Debug)]
pub enum ErrorKind {
    Msg(String),
    Wrapped(Arc<dyn std::error::Error + Send + Sync>),
}

/// A diagnostic error with location info
#[derive(Clone, Debug)]
pub struct Error {
    kind: ErrorKind,
    location: ErrorLocation,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.kind {
            ErrorKind::Msg(s) => write!(f, "{}", s),
            ErrorKind::Wrapped(e) => write!(f, "{}", e),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match &self.kind {
            ErrorKind::Msg(_) => None,
            ErrorKind::Wrapped(e) => e.source(),
        }
    }
}

impl miette::Diagnostic for Error {
    fn help<'a>(&'a self) -> Option<Box<dyn std::fmt::Display + 'a>> {
        if self.location.file.is_empty() && self.location.path.is_empty() {
            None
        } else if self.location.path.is_empty() {
            Some(Box::new(format!("at {}", self.location.file)))
        } else {
            Some(Box::new(format!(
                "at {}:{}",
                self.location.file, self.location.path
            )))
        }
    }
}

impl Error {
    /// Create from a message
    pub fn msg(msg: impl Into<String>) -> Self {
        Error {
            kind: ErrorKind::Msg(msg.into()),
            location: ErrorLocation::default(),
        }
    }

    /// Add file location
    pub fn at_file(mut self, file: impl Into<String>) -> Self {
        self.location.file = file.into();
        self
    }

    /// Add path location
    pub fn at_path(mut self, path: impl Into<String>) -> Self {
        self.location.path = path.into();
        self
    }

    /// Add full location
    pub fn at(mut self, file: impl Into<String>, path: impl Into<String>) -> Self {
        self.location.file = file.into();
        self.location.path = path.into();
        self
    }

    /// Wrap an error
    pub fn wrap<E: std::error::Error + Send + Sync + 'static>(err: E) -> Self {
        Error {
            kind: ErrorKind::Wrapped(Arc::new(err)),
            location: ErrorLocation::default(),
        }
    }
}

impl PipelineValue {
    pub fn try_into_string(self) -> Result<String, Error> {
        match self {
            PipelineValue::String(x) => Ok(x),
            _ => Err(Error::msg("Could not convert input to string")),
        }
    }

    pub fn try_into_bytes(self) -> Result<Vec<u8>, Error> {
        match self {
            PipelineValue::Bytes(x) => Ok(x),
            _ => Err(Error::msg("Could not convert input to bytes")),
        }
    }

    pub fn try_into_json(self) -> Result<serde_json::Value, Error> {
        match self {
            PipelineValue::Json(x) => Ok(x),
            _ => Err(Error::msg("Could not convert input to json")),
        }
    }

    pub fn try_into_audio(self) -> Result<AudioBuffer, Error> {
        match self {
            PipelineValue::Audio(x) => Ok(x),
            _ => Err(Error::msg("Could not convert input to audio")),
        }
    }
}

impl From<String> for PipelineValue {
    fn from(value: String) -> Self {
        PipelineValue::String(value)
    }
}

impl From<Vec<u8>> for PipelineValue {
    fn from(value: Vec<u8>) -> Self {
        PipelineValue::Bytes(value)
    }
}

impl From<serde_json::Value> for PipelineValue {
    fn from(value: serde_json::Value) -> Self {
        PipelineValue::Json(value)
    }
}

impl From<AudioBuffer> for PipelineValue {
    fn from(value: AudioBuffer) -> Self {
        PipelineValue::Audio(value)
    }
}

// --- Convenience conversions so a `forward` impl can keep writing
//     `Ok(x.into())` for a single output, regardless of x's underlying type.
impl From<PipelineValue> for PipelineValues {
    fn from(v: PipelineValue) -> Self {
        PipelineValues(vec![v])
    }
}

impl From<String> for PipelineValues {
    fn from(s: String) -> Self {
        PipelineValues(vec![PipelineValue::String(s)])
    }
}

impl From<Vec<u8>> for PipelineValues {
    fn from(b: Vec<u8>) -> Self {
        PipelineValues(vec![PipelineValue::Bytes(b)])
    }
}

impl From<serde_json::Value> for PipelineValues {
    fn from(v: serde_json::Value) -> Self {
        PipelineValues(vec![PipelineValue::Json(v)])
    }
}

impl From<AudioBuffer> for PipelineValues {
    fn from(audio: AudioBuffer) -> Self {
        PipelineValues(vec![PipelineValue::Audio(audio)])
    }
}

impl From<Vec<String>> for PipelineValues {
    fn from(v: Vec<String>) -> Self {
        PipelineValues(v.into_iter().map(PipelineValue::String).collect())
    }
}

impl From<Vec<PipelineValue>> for PipelineValues {
    fn from(v: Vec<PipelineValue>) -> Self {
        PipelineValues(v)
    }
}

pub enum DataRef {
    BoxFile(Box<BoxFileReader>),
    Path(PathBuf),
}

pub struct Context {
    pub(crate) data: DataRef,
    pub dev: bool,
    pub base_path: Option<PathBuf>,
}

impl Context {
    pub async fn load_pipeline_bundle(&self) -> Result<PipelineBundle, Error> {
        let bundle: PipelineBundle = match &self.data {
            DataRef::BoxFile(bf) => {
                let record = bf
                    .find(
                        &BoxPath::new("pipeline.json")
                            .map_err(|e| Error::wrap(e).at_file("pipeline.json"))?,
                    )
                    .map_err(|e| Error::wrap(e).at_file("pipeline.json"))?
                    .as_file()
                    .unwrap();
                let json: serde_json::Value = if record.compression == Compression::Stored {
                    let m = bf
                        .memory_map(&record)
                        .map_err(|e| Error::wrap(e).at_file("pipeline.json"))?;
                    serde_json::from_slice(
                        m.as_slice()
                            .map_err(|e| Error::wrap(e).at_file("pipeline.json"))?,
                    )
                    .map_err(|e| Error::wrap(e).at_file("pipeline.json"))?
                } else {
                    let mut buf = Vec::with_capacity(record.decompressed_length as _);
                    let mut reader = bf
                        .read_bytes(record)
                        .await
                        .map_err(|e| Error::wrap(e).at_file("pipeline.json"))?;
                    reader
                        .read_to_end(&mut buf)
                        .await
                        .map_err(|e| Error::wrap(e).at_file("pipeline.json"))?;
                    serde_json::from_slice(&buf)
                        .map_err(|e| Error::wrap(e).at_file("pipeline.json"))?
                };
                PipelineBundle::from_json(json)
                    .map_err(|e| Error::wrap(e).at_file("pipeline.json"))?
            }
            DataRef::Path(p) => {
                let p = p.join("pipeline.json");
                let contents = tokio::fs::read(&p)
                    .await
                    .map_err(|e| Error::wrap(e).at_file(p.display().to_string()))?;
                let json: serde_json::Value = serde_json::from_slice(&contents)
                    .map_err(|e| Error::wrap(e).at_file(p.display().to_string()))?;
                PipelineBundle::from_json(json)
                    .map_err(|e| Error::wrap(e).at_file(p.display().to_string()))?
            }
        };

        Ok(bundle)
    }

    pub async fn load_pipeline_definition(&self) -> Result<PipelineDefinition, Error> {
        let bundle = self.load_pipeline_bundle().await?;
        self.enrich_pipeline(
            bundle
                .get_pipeline(None)
                .ok_or_else(|| Error::msg("No default pipeline found").at_file("pipeline.json"))?
                .clone(),
        )
    }

    pub async fn load_pipeline_definition_named(
        &self,
        name: &str,
    ) -> Result<PipelineDefinition, Error> {
        let bundle = self.load_pipeline_bundle().await?;
        self.enrich_pipeline(
            bundle
                .get_pipeline(Some(name))
                .ok_or_else(|| {
                    Error::msg(format!("Pipeline '{}' not found", name)).at_file("pipeline.json")
                })?
                .clone(),
        )
    }

    fn enrich_pipeline(
        &self,
        mut pipeline: PipelineDefinition,
    ) -> Result<PipelineDefinition, Error> {
        let module_map = get_modules()
            .iter()
            .map(|x| x.commands.iter().map(|cmd| ((x.name, cmd.name), cmd)))
            .flatten()
            .collect::<HashMap<_, _>>();

        // Enrich commands with metadata from CommandDefs
        for (_key, command) in pipeline.commands.iter_mut() {
            // If kind is not set in JSON, copy from CommandDef
            if command.kind.is_none() {
                if let Some(cmd_def) =
                    module_map.get(&(command.module.as_str(), command.command.as_str()))
                {
                    if let Some(kind) = cmd_def.kind {
                        command.kind = Some(kind.to_string());
                    }
                }
            }
        }

        Ok(pipeline)
    }

    fn resolve_path(&self, path: &str) -> Result<PathBuf, Error> {
        if path.starts_with('@') {
            // @ prefix - only allowed in dev mode
            if !self.dev {
                return Err(
                    Error::msg("@ prefix paths are only allowed in dev pipelines").at_file(path),
                );
            }
            // Drop the @ and resolve relative to pipeline.ts location
            let relative_path = &path[1..];
            let base = self
                .base_path
                .as_ref()
                .ok_or_else(|| Error::msg("base_path not set for dev context"))?;
            Ok(base.join(relative_path))
        } else {
            // Regular path - loads from assets/
            match &self.data {
                DataRef::BoxFile(_) => Ok(PathBuf::from(path)),
                DataRef::Path(p) => Ok(p.join("assets").join(path)),
            }
        }
    }

    /// Load a divvun-fst model from either the ordinary assets directory or
    /// directly from records in a bundle. The returned transducer owns its
    /// mappings, so the temporary synchronous bundle reader can be dropped.
    pub(crate) fn load_fst<T>(&self, path: impl AsRef<Path>) -> Result<T, Error>
    where
        T: divvun_fst::transducer::TransducerLoader<std::fs::File>
            + divvun_fst::transducer::TransducerLoader<divvun_fst::vfs::boxf::File>,
    {
        let path_str = path
            .as_ref()
            .to_str()
            .ok_or_else(|| Error::msg("Invalid path"))?;
        let resolved = self.resolve_path(path_str)?;

        match &self.data {
            DataRef::BoxFile(bf) if !path_str.starts_with('@') => {
                let reader = box_format::sync::BoxReader::open(bf.path())
                    .map_err(|e| Error::wrap(e).at_file(resolved.display().to_string()))?;
                let fs = divvun_fst::vfs::boxf::Filesystem::new(&reader);
                T::from_path(&fs, &resolved)
                    .map_err(|e| Error::wrap(e).at_file(resolved.display().to_string()))
            }
            _ => T::from_path(&divvun_fst::vfs::Fs, &resolved)
                .map_err(|e| Error::wrap(e).at_file(resolved.display().to_string())),
        }
    }

    pub async fn load_file(&self, path: impl AsRef<Path>) -> Result<Vec<u8>, Error> {
        let path_str = path
            .as_ref()
            .to_str()
            .ok_or_else(|| Error::msg("Invalid path"))?;
        let resolved = self.resolve_path(path_str)?;

        match &self.data {
            DataRef::BoxFile(bf) => {
                tracing::debug!("Loading file from box file: {}", resolved.display());
                let record = bf
                    .find(
                        &BoxPath::new(&resolved)
                            .map_err(|e| Error::wrap(e).at_file(resolved.display().to_string()))?,
                    )
                    .map_err(|e| Error::wrap(e).at_file(resolved.display().to_string()))?
                    .as_file()
                    .unwrap();
                let mut reader = bf
                    .read_bytes(record)
                    .await
                    .map_err(|e| Error::wrap(e).at_file(resolved.display().to_string()))?;
                let mut buf = Vec::new();
                reader
                    .read_to_end(&mut buf)
                    .await
                    .map_err(|e| Error::wrap(e).at_file(resolved.display().to_string()))?;
                Ok(buf)
            }
            DataRef::Path(_) => {
                tracing::debug!("Loading file from path: {}", resolved.display());
                tokio::fs::read(&resolved)
                    .await
                    .map_err(|e| Error::wrap(e).at_file(resolved.display().to_string()))
            }
        }
    }

    pub async fn load_file_optional(
        &self,
        path: impl AsRef<Path>,
    ) -> Result<Option<Vec<u8>>, Error> {
        let path_str = path
            .as_ref()
            .to_str()
            .ok_or_else(|| Error::msg("Invalid path"))?;
        let resolved = self.resolve_path(path_str)?;

        match &self.data {
            DataRef::BoxFile(bf) if !path_str.starts_with('@') => {
                let bpath = BoxPath::new(&resolved)
                    .map_err(|e| Error::wrap(e).at_file(resolved.display().to_string()))?;
                let Some(index) = bf.metadata().index(&bpath) else {
                    return Ok(None);
                };
                let record = bf
                    .metadata()
                    .record(index)
                    .and_then(|record| record.as_file())
                    .ok_or_else(|| {
                        Error::msg("Not a file").at_file(resolved.display().to_string())
                    })?;
                let mut reader = bf
                    .read_bytes(record)
                    .await
                    .map_err(|e| Error::wrap(e).at_file(resolved.display().to_string()))?;
                let mut buf = Vec::new();
                reader
                    .read_to_end(&mut buf)
                    .await
                    .map_err(|e| Error::wrap(e).at_file(resolved.display().to_string()))?;
                Ok(Some(buf))
            }
            _ => match tokio::fs::read(&resolved).await {
                Ok(contents) => Ok(Some(contents)),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
                Err(e) => Err(Error::wrap(e).at_file(resolved.display().to_string())),
            },
        }
    }

    pub async fn load_files_glob(&self, pattern: &str) -> Result<Vec<(PathBuf, Vec<u8>)>, Error> {
        match &self.data {
            DataRef::BoxFile(bf) => {
                // For box files, we need to iterate through entries and match the pattern
                let mut files = Vec::new();
                for entry in bf.metadata().iter() {
                    let path_str = entry.path.to_string();
                    if glob_match(pattern, &path_str) {
                        if let Some(file_record) = entry.record.as_file() {
                            let mut reader = bf
                                .read_bytes(file_record)
                                .await
                                .map_err(|e| Error::wrap(e).at_file(&path_str))?;
                            let mut buf = Vec::new();
                            reader
                                .read_to_end(&mut buf)
                                .await
                                .map_err(|e| Error::wrap(e).at_file(&path_str))?;
                            files.push((PathBuf::from(path_str), buf));
                        }
                    }
                }
                Ok(files)
            }
            DataRef::Path(p) => {
                // For regular paths, use filesystem globbing
                let assets_dir = p.join("assets");
                let full_pattern = assets_dir.join(pattern);
                let mut files = Vec::new();

                for entry in glob::glob(full_pattern.to_str().unwrap())
                    .map_err(|e| Error::wrap(e).at_file(pattern))?
                {
                    let path = entry.map_err(Error::wrap)?;
                    if path.is_file() {
                        let contents = tokio::fs::read(&path)
                            .await
                            .map_err(|e| Error::wrap(e).at_file(path.display().to_string()))?;
                        files.push((path, contents));
                    }
                }
                Ok(files)
            }
        }
    }

    pub async fn memory_map_file(&self, path: impl AsRef<Path>) -> Result<Segment, Error> {
        let path_str = path
            .as_ref()
            .to_str()
            .ok_or_else(|| Error::msg("Invalid path"))?;
        let resolved = self.resolve_path(path_str)?;
        let path_display = resolved.display().to_string();
        match &self.data {
            DataRef::BoxFile(bf) if !path_str.starts_with('@') => {
                tracing::debug!("Memory mapping file from box: {}", resolved.display());
                let bpath =
                    BoxPath::new(&resolved).map_err(|e| Error::wrap(e).at_file(&path_display))?;
                let node = bf
                    .find(&bpath)
                    .map_err(|e| Error::wrap(e).at_file(&path_display))?;
                let node = node
                    .as_file()
                    .ok_or_else(|| Error::msg("Not a file").at_file(&path_display))?;

                bf.memory_map(node)
                    .map_err(|e| Error::wrap(e).at_file(&path_display))
            }
            _ => {
                tracing::debug!("Memory mapping file: {}", resolved.display());
                let full_path_clone = resolved.clone();
                tokio::task::spawn_blocking(move || {
                    let mmap =
                        Arc::new(MemoryMappedFile::open_ro(&full_path_clone).map_err(|e| {
                            Error::wrap(e).at_file(full_path_clone.display().to_string())
                        })?);
                    let len = mmap.len();
                    Segment::new(mmap, 0, len)
                        .map_err(|e| Error::wrap(e).at_file(full_path_clone.display().to_string()))
                })
                .await
                .map_err(|e| Error::wrap(e).at_file(resolved.display().to_string()))?
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Module {
    pub name: &'static str,
    pub commands: &'static [CommandDef],
}

impl Display for Module {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Module: ")?;
        f.write_str(self.name)?;
        f.write_str("\n")?;
        for command in self.commands {
            f.write_str("  ")?;
            f.write_str(command.name)?;
            f.write_char('\n')?;
        }
        Ok(())
    }
}

pub type InitFn = fn(
    Arc<Context>,
    HashMap<String, ast::Arg>,
) -> Pin<
    Box<dyn Future<Output = Result<Arc<dyn CommandRunner + Send + Sync>, Error>> + Send>,
>;

#[derive(Debug, Clone)]
pub struct CommandDef {
    pub name: &'static str,
    pub module: &'static str,
    pub input: &'static [Ty],
    pub args: &'static [Arg],
    pub assets: &'static [AssetDep],
    pub init: InitFn,
    pub returns: Ty,
    pub kind: Option<&'static str>,
    pub schema: Option<&'static str>,
    pub config: Option<&'static str>,
    pub shape: Option<&'static facet::Shape>,
    pub config_shape: Option<&'static facet::Shape>,
}

#[derive(Debug, Clone)]
pub struct Arg {
    pub name: &'static str,
    pub ty: Ty,
    pub optional: bool,
}

#[derive(Debug, Clone)]
pub struct StructDef {
    pub name: &'static str,
    pub module: &'static str,
    pub fields: &'static [StructField],
    pub shape: Option<&'static facet::Shape>,
}

#[derive(Debug, Clone)]
pub struct StructField {
    pub name: &'static str,
    pub ty: &'static str,
    pub optional: bool,
}

#[derive(Debug, Clone)]
pub enum AssetDep {
    Required(&'static str),      // required("file.json")
    RequiredRegex(&'static str), // required(r"pattern")
    Optional(&'static str),      // optional("file.json")
    OptionalRegex(&'static str), // optional(r"pattern")
}

#[derive(Debug, Clone)]
pub enum Ty {
    Path,
    String,
    Json,
    Bytes,
    Int,
    ArrayString,
    ArrayBytes,
    MapPath,
    MapString,
    MapBytes,
    Struct(&'static str), // Custom struct type with name
    Union(Vec<Ty>),       // For supporting multiple types (replacing bitmask functionality)
}

impl FromStr for Ty {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.starts_with('[') && s.ends_with(']') {
            let inner = Ty::from_str(&s[1..s.len() - 1])?;
            return if matches!(inner, Ty::String) {
                Ok(Ty::ArrayString)
            } else if matches!(inner, Ty::Bytes) {
                Ok(Ty::ArrayBytes)
            } else {
                Err(())
            };
        }

        if s.starts_with("{") && s.ends_with("}") {
            let inner = Ty::from_str(&s[1..s.len() - 1])?;
            return if matches!(inner, Ty::Path) {
                Ok(Ty::MapPath)
            } else if matches!(inner, Ty::String) {
                Ok(Ty::MapString)
            } else if matches!(inner, Ty::Bytes) {
                Ok(Ty::MapBytes)
            } else {
                Err(())
            };
        }

        match s {
            "path" => Ok(Ty::Path),
            "string" => Ok(Ty::String),
            "json" => Ok(Ty::Json),
            "bytes" => Ok(Ty::Bytes),
            "int" => Ok(Ty::Int),
            _ => Err(()),
        }
    }
}

impl Ty {
    pub fn as_dr_type(&self) -> Cow<'static, str> {
        match self {
            Ty::Path => "path".into(),
            Ty::String => "string".into(),
            Ty::Json => "json".into(),
            Ty::Bytes => "bytes".into(),
            Ty::Int => "int".into(),
            Ty::ArrayString => "[string]".into(),
            Ty::ArrayBytes => "[bytes]".into(),
            Ty::MapPath => "{path}".into(),
            Ty::MapString => "{string}".into(),
            Ty::MapBytes => "{bytes}".into(),
            Ty::Struct(name) => Cow::Owned(name.to_string()),
            Ty::Union(types) => {
                let type_strs: Vec<_> = types.iter().map(|t| t.as_dr_type()).collect();
                Cow::Owned(type_strs.join(" | "))
            }
        }
    }
}

inventory::collect!(&'static CommandDef);
inventory::collect!(&'static StructDef);

static MODULES: Lazy<Vec<Module>> = Lazy::new(|| {
    let mut modules_map: HashMap<&str, Vec<&CommandDef>> = HashMap::new();

    // Group commands by module
    for command_def in inventory::iter::<&CommandDef>() {
        modules_map
            .entry(command_def.module)
            .or_insert_with(Vec::new)
            .push(command_def);
    }

    // Convert to Module structs
    let mut modules = Vec::new();
    for (module_name, command_defs) in modules_map {
        // Convert Vec<&CommandDef> to &'static [CommandDef]
        let commands: Vec<CommandDef> = command_defs.into_iter().cloned().collect();
        let commands_slice = Box::leak(commands.into_boxed_slice());

        modules.push(Module {
            name: module_name,
            commands: commands_slice,
        });
    }

    modules.sort_by(|a, b| a.name.cmp(b.name));
    modules
});

pub fn get_modules() -> &'static Vec<Module> {
    &*MODULES
}

pub fn get_structs() -> impl Iterator<Item = &'static StructDef> {
    inventory::iter::<&StructDef>().copied()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TapOutput {
    #[default]
    Continue,
    Stop,
}

pub type TapFn = dyn Fn(&str, &Command, &PipelineEvent) -> Pin<Box<dyn Future<Output = TapOutput> + Send>>
    + Send
    + Sync;

#[derive(Clone)]
pub struct Tap {
    pub key: Arc<str>,
    pub command: Arc<Command>,
    pub tap: Arc<TapFn>,
}

#[async_trait]
pub trait CommandRunner: Any
where
    Self: 'static,
{
    async fn forward(
        self: Arc<Self>,
        input: PipelineValue,
        _config: Arc<serde_json::Value>,
    ) -> Result<PipelineValues, Error> {
        Ok(input.into())
    }

    fn forward_stream(
        self: Arc<Self>,
        mut input_rx: PipelineValueRx,
        output: PipelineValueTx,
        tap: Option<Tap>,
        config: Arc<serde_json::Value>,
    ) -> JoinHandle<Result<(), Error>>
    where
        Self: Send + Sync + 'static,
    {
        let this = self.clone();
        let name = self.name().to_string();
        tokio::spawn(async move {
            tracing::debug!("{name}: forward_stream task started");
            loop {
                let event = input_rx.recv().await.map_err(Error::wrap)?;
                let this = this.clone();
                match event {
                    PipelineEvent::Value(input) => {
                        tracing::debug!("{name}: received input, forwarding");
                        let outputs = match this.forward(input, config.clone()).await {
                            Ok(outputs) => {
                                tracing::debug!(
                                    "{name}: forward produced {} value(s)",
                                    outputs.0.len()
                                );
                                outputs
                            }
                            Err(e) => {
                                tracing::error!("{name}: forward error: {e:?}");
                                output
                                    .send(PipelineEvent::Error(e.clone()))
                                    .map_err(Error::wrap)?;
                                return Err(e);
                            }
                        };

                        let mut stopped = false;
                        for value in outputs {
                            let event = PipelineEvent::Value(value);
                            if let Some(tap) = &tap {
                                let tap_output = (tap.tap)(&tap.key, &tap.command, &event).await;
                                match tap_output {
                                    TapOutput::Continue => {}
                                    TapOutput::Stop => {
                                        stopped = true;
                                        break;
                                    }
                                }
                            }
                            output.send(event).map_err(Error::wrap)?;
                        }
                        if stopped {
                            continue;
                        }
                    }
                    PipelineEvent::Finish => {
                        tracing::trace!("{name}: received Finish");
                        output.send(PipelineEvent::Finish).map_err(Error::wrap)?;
                    }
                    PipelineEvent::Error(e) => {
                        tracing::error!("{name}: received Error: {e:?}");
                        output
                            .send(PipelineEvent::Error(e.clone()))
                            .map_err(Error::wrap)?;
                        return Err(e);
                    }
                    PipelineEvent::Cancel => {
                        tracing::debug!("{name}: received Cancel");
                        // Stateless commands have no in-flight work to drop; just
                        // forward the signal and keep listening. Streaming commands
                        // override forward_stream to abort their inner emission.
                        output.send(PipelineEvent::Cancel).map_err(Error::wrap)?;
                    }
                    PipelineEvent::Close => {
                        tracing::debug!("{name}: received Close");
                        output.send(PipelineEvent::Close).map_err(Error::wrap)?;
                        break;
                    }
                }
            }
            Ok(())
        })
    }

    fn name(&self) -> &'static str;
}

#[cfg(test)]
mod context_tests {
    use super::*;

    #[test]
    fn audio_buffer_serializes_as_float_wav() {
        let audio = AudioBuffer {
            samples: vec![-0.5, 0.25],
            sample_rate: 22_050,
            channels: 1,
            word_timings: vec![AudioWordTiming {
                word: "test".into(),
                start_sample: 0,
                end_sample: 2,
            }],
        };
        let wav = audio.to_wav_bytes().unwrap();
        let mut reader = hound::WavReader::new(std::io::Cursor::new(wav)).unwrap();

        assert_eq!(reader.spec().sample_rate, 22_050);
        assert_eq!(reader.spec().channels, 1);
        assert_eq!(reader.spec().bits_per_sample, 32);
        assert_eq!(reader.spec().sample_format, hound::SampleFormat::Float);
        assert_eq!(
            reader
                .samples::<f32>()
                .collect::<Result<Vec<_>, _>>()
                .unwrap(),
            audio.samples
        );
    }

    #[tokio::test]
    async fn memory_map_file_resolves_asset_and_dev_paths() {
        let temp = tempfile::tempdir().unwrap();
        let assets = temp.path().join("assets");
        std::fs::create_dir(&assets).unwrap();
        std::fs::write(assets.join("model.bin"), b"asset model").unwrap();
        std::fs::write(temp.path().join("dev-model.bin"), b"dev model").unwrap();

        let context = Context {
            data: DataRef::Path(temp.path().to_path_buf()),
            dev: true,
            base_path: Some(temp.path().to_path_buf()),
        };

        let asset = context.memory_map_file("model.bin").await.unwrap();
        assert_eq!(&*asset.as_slice().unwrap(), b"asset model");

        let dev = context.memory_map_file("@dev-model.bin").await.unwrap();
        assert_eq!(&*dev.as_slice().unwrap(), b"dev model");

        assert_eq!(
            context.load_file_optional("model.bin").await.unwrap(),
            Some(b"asset model".to_vec())
        );
        assert_eq!(
            context.load_file_optional("@dev-model.bin").await.unwrap(),
            Some(b"dev model".to_vec())
        );
        assert_eq!(
            context.load_file_optional("missing.bin").await.unwrap(),
            None
        );
    }
}
