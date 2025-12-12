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
use tempfile::TempDir;
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

pub type InputFut = Pin<Box<dyn Future<Output = Result<Input, Error>> + Send>>;
pub type SharedInputFut = SharedBox<dyn Future<Output = Result<Input, Error>> + Send>;

#[derive(Debug, Clone)]
pub enum InputEvent {
    Input(Input),
    Error(Error),
    Finish,
    Close,
}

pub type InputTx = Sender<InputEvent>;
pub type InputRx = Receiver<InputEvent>;

#[derive(Debug, Clone)]
pub enum Input {
    String(String),
    Bytes(Vec<u8>),
    ArrayString(Vec<String>),
    ArrayBytes(Vec<Vec<u8>>),
    Json(serde_json::Value),
    Multiple(Box<[Input]>),
}

impl Display for InputEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InputEvent::Input(x) => write!(f, "{:#}", x)?,
            InputEvent::Error(x) => write!(f, "Error: {:#}", x)?,
            InputEvent::Finish => write!(f, "Finish")?,
            InputEvent::Close => write!(f, "Close")?,
        }
        Ok(())
    }
}

impl Display for Input {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if f.alternate() {
            match self {
                Input::String(x) => write!(f, "{}", x)?,
                Input::Bytes(x) => write!(f, "<<{} bytes>>", x.len())?,
                Input::ArrayString(x) => write!(f, "{:#?}", x)?,
                Input::ArrayBytes(x) => {
                    write!(f, "[")?;
                    for (_i, x) in x.iter().enumerate() {
                        write!(f, "<<{} bytes>>,", x.len())?;
                    }
                    write!(f, "]")?;
                }
                Input::Json(x) => write!(f, "{}", serde_json::to_string_pretty(&x).unwrap())?,
                Input::Multiple(x) => {
                    writeln!(f, "[")?;
                    for (i, x) in x.iter().enumerate() {
                        writeln!(f, "{i}: {}", x)?;
                    }
                    write!(f, "]")?;
                }
            }
            return Ok(());
        }

        match self {
            Input::String(x) => write!(f, "{}", x),
            Input::Bytes(x) => write!(f, "<<{} bytes>>", x.len()),
            Input::ArrayString(x) => write!(f, "[{}]", x.join(", ")),
            Input::ArrayBytes(x) => write!(
                f,
                "[{}]",
                x.iter()
                    .map(|x| format!("<<{} bytes>>", x.len()))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Input::Json(x) => write!(f, "{}", serde_json::to_string(&x).unwrap()),
            Input::Multiple(x) => write!(
                f,
                "[{}]",
                x.iter()
                    .map(|x| format!("{}", x))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
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
#[derive(Clone, Debug, miette::Diagnostic)]
#[diagnostic()]
pub struct Error {
    kind: ErrorKind,
    location: ErrorLocation,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.kind {
            ErrorKind::Msg(s) => write!(f, "{}{}", s, self.location),
            ErrorKind::Wrapped(e) => write!(f, "{}{}", e, self.location),
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

impl Input {
    pub fn try_into_string(self) -> Result<String, Error> {
        match self {
            Input::String(x) => Ok(x),
            _ => Err(Error::msg("Could not convert input to string")),
        }
    }

    pub fn try_into_bytes(self) -> Result<Vec<u8>, Error> {
        match self {
            Input::Bytes(x) => Ok(x),
            _ => Err(Error::msg("Could not convert input to bytes")),
        }
    }

    pub fn try_into_json(self) -> Result<serde_json::Value, Error> {
        match self {
            Input::Json(x) => Ok(x),
            _ => Err(Error::msg("Could not convert input to json")),
        }
    }

    pub fn try_into_string_array(self) -> Result<Vec<String>, Error> {
        match self {
            Input::ArrayString(x) => Ok(x),
            _ => Err(Error::msg("Could not convert input to string array")),
        }
    }

    pub fn try_into_bytes_array(self) -> Result<Vec<Vec<u8>>, Error> {
        match self {
            Input::ArrayBytes(x) => Ok(x),
            _ => Err(Error::msg("Could not convert input to bytes array")),
        }
    }

    pub fn try_into_multiple(self) -> Result<Box<[Input]>, Error> {
        match self {
            Input::Multiple(x) => Ok(x),
            _ => Err(Error::msg("Could not convert input to multiple")),
        }
    }
}

impl From<String> for Input {
    fn from(value: String) -> Self {
        Input::String(value)
    }
}

impl From<Vec<u8>> for Input {
    fn from(value: Vec<u8>) -> Self {
        Input::Bytes(value)
    }
}

impl From<serde_json::Value> for Input {
    fn from(value: serde_json::Value) -> Self {
        Input::Json(value)
    }
}

impl From<Vec<String>> for Input {
    fn from(value: Vec<String>) -> Self {
        Input::ArrayString(value)
    }
}

pub enum DataRef {
    BoxFile(Box<BoxFileReader>, TempDir),
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
            DataRef::BoxFile(bf, _) => {
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
                DataRef::BoxFile(_, _) => Ok(PathBuf::from(path)),
                DataRef::Path(p) => Ok(p.join("assets").join(path)),
            }
        }
    }

    pub async fn load_file(&self, path: impl AsRef<Path>) -> Result<Vec<u8>, Error> {
        let path_str = path
            .as_ref()
            .to_str()
            .ok_or_else(|| Error::msg("Invalid path"))?;
        let resolved = self.resolve_path(path_str)?;

        match &self.data {
            DataRef::BoxFile(bf, _) => {
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

    pub async fn extract_to_temp_dir(&self, path: impl AsRef<Path>) -> Result<PathBuf, Error> {
        let path_str = path
            .as_ref()
            .to_str()
            .ok_or_else(|| Error::msg("Invalid path"))?;
        let resolved = self.resolve_path(path_str)?;

        match &self.data {
            DataRef::BoxFile(bf, tmp) => {
                tracing::debug!("Extracting file to temp dir: {}", resolved.display());
                let bpath = BoxPath::new(&resolved)
                    .map_err(|e| Error::wrap(e).at_file(resolved.display().to_string()))?;
                bf.extract_recursive(&bpath, tmp.path())
                    .await
                    .map_err(|e| Error::wrap(e).at_file(resolved.display().to_string()))?;
                Ok(tmp.path().join(&resolved))
            }
            DataRef::Path(_) => Ok(resolved),
        }
    }

    pub async fn load_files_glob(&self, pattern: &str) -> Result<Vec<(PathBuf, Vec<u8>)>, Error> {
        match &self.data {
            DataRef::BoxFile(bf, _tmp) => {
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
        let path_display = path.as_ref().display().to_string();
        match &self.data {
            DataRef::BoxFile(bf, _) => {
                tracing::debug!("Memory mapping file from box: {}", path.as_ref().display());
                let bpath = BoxPath::new(path.as_ref())
                    .map_err(|e| Error::wrap(e).at_file(&path_display))?;
                let node = bf
                    .find(&bpath)
                    .map_err(|e| Error::wrap(e).at_file(&path_display))?;
                let node = node
                    .as_file()
                    .ok_or_else(|| Error::msg("Not a file").at_file(&path_display))?;

                bf.memory_map(node)
                    .map_err(|e| Error::wrap(e).at_file(&path_display))
            }
            DataRef::Path(p) => {
                let full_path = p.join("assets").join(&path);
                tracing::debug!("Memory mapping file: {}", full_path.display());
                let full_path_clone = full_path.clone();
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
                .map_err(|e| Error::wrap(e).at_file(full_path.display().to_string()))?
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

pub type TapFn = dyn Fn(&str, &Command, &InputEvent) -> Pin<Box<dyn Future<Output = TapOutput> + Send>>
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
        input: Input,
        _config: Arc<serde_json::Value>,
    ) -> Result<Input, Error> {
        Ok(input)
    }

    fn forward_stream(
        self: Arc<Self>,
        mut input_rx: InputRx,
        output: InputTx,
        tap: Option<Tap>,
        config: Arc<serde_json::Value>,
    ) -> JoinHandle<Result<(), Error>>
    where
        Self: Send + Sync + 'static,
    {
        let this = self.clone();
        tokio::spawn(async move {
            loop {
                let event = input_rx.recv().await.map_err(Error::wrap)?;
                let this = this.clone();
                match event {
                    InputEvent::Input(input) => {
                        let event = match this.forward(input, config.clone()).await {
                            Ok(output) => InputEvent::Input(output),
                            Err(e) => {
                                output
                                    .send(InputEvent::Error(e.clone()))
                                    .map_err(Error::wrap)?;
                                return Err(e);
                            }
                        };

                        if let Some(tap) = &tap {
                            let tap_output = (tap.tap)(&tap.key, &tap.command, &event).await;
                            match tap_output {
                                TapOutput::Continue => {}
                                TapOutput::Stop => {
                                    output.send(InputEvent::Finish).map_err(Error::wrap)?;
                                    continue;
                                }
                            }
                        }

                        output.send(event).map_err(Error::wrap)?;
                        output.send(InputEvent::Finish).map_err(Error::wrap)?;
                    }
                    InputEvent::Finish => {
                        output.send(InputEvent::Finish).map_err(Error::wrap)?;
                    }
                    InputEvent::Error(e) => {
                        output
                            .send(InputEvent::Error(e.clone()))
                            .map_err(Error::wrap)?;
                        return Err(e);
                    }
                    InputEvent::Close => {
                        output.send(InputEvent::Close).map_err(Error::wrap)?;
                        break;
                    }
                }
            }
            Ok(())
        })
    }

    fn name(&self) -> &'static str;
}
