use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use ast::{Command, Pipe, PipelineDefinition, PipelineHandle};
use modules::{Context, Input, InputEvent, Module};

use box_format::OpenError;
use tempfile::TempDir;

pub mod ast;
pub mod modules;
pub mod ts;
mod util;

pub enum BundleContentsPath {
    TempDir(TempDir),
    Literal(PathBuf),
}

impl BundleContentsPath {
    pub fn path(&self) -> &Path {
        match self {
            BundleContentsPath::TempDir(p) => p.path(),
            BundleContentsPath::Literal(p) => p,
        }
    }
}

#[derive(Debug)]
pub(crate) struct VersionInfo {
    build_date: &'static str,
    build_timestamp: &'static str,
    cargo_debug: &'static str,
    cargo_features: &'static str,
    cargo_opt_level: &'static str,
    cargo_target_triple: &'static str,
    cargo_dependencies: &'static str,
    rustc_channel: &'static str,
    rustc_commit_date: &'static str,
    rustc_commit_hash: &'static str,
    rustc_host_triple: &'static str,
    rustc_llvm_version: &'static str,
    rustc_semver: &'static str,
    git_branch: &'static str,
    git_commit_author_email: &'static str,
    git_commit_author_name: &'static str,
    git_commit_count: &'static str,
    git_commit_date: &'static str,
    git_commit_message: &'static str,
    git_commit_timestamp: &'static str,
    git_describe: &'static str,
}

pub const VERSION_INFO: VersionInfo = VersionInfo {
    build_date: env!("VERGEN_BUILD_DATE"),
    build_timestamp: env!("VERGEN_BUILD_TIMESTAMP"),
    cargo_debug: env!("VERGEN_CARGO_DEBUG"),
    cargo_features: env!("VERGEN_CARGO_FEATURES"),
    cargo_opt_level: env!("VERGEN_CARGO_OPT_LEVEL"),
    cargo_target_triple: env!("VERGEN_CARGO_TARGET_TRIPLE"),
    cargo_dependencies: env!("VERGEN_CARGO_DEPENDENCIES"),
    rustc_channel: env!("VERGEN_RUSTC_CHANNEL"),
    rustc_commit_date: env!("VERGEN_RUSTC_COMMIT_DATE"),
    rustc_commit_hash: env!("VERGEN_RUSTC_COMMIT_HASH"),
    rustc_host_triple: env!("VERGEN_RUSTC_HOST_TRIPLE"),
    rustc_llvm_version: env!("VERGEN_RUSTC_LLVM_VERSION"),
    rustc_semver: env!("VERGEN_RUSTC_SEMVER"),
    git_branch: env!("VERGEN_GIT_BRANCH"),
    git_commit_author_email: env!("VERGEN_GIT_COMMIT_AUTHOR_EMAIL"),
    git_commit_author_name: env!("VERGEN_GIT_COMMIT_AUTHOR_NAME"),
    git_commit_count: env!("VERGEN_GIT_COMMIT_COUNT"),
    git_commit_date: env!("VERGEN_GIT_COMMIT_DATE"),
    git_commit_message: env!("VERGEN_GIT_COMMIT_MESSAGE"),
    git_commit_timestamp: env!("VERGEN_GIT_COMMIT_TIMESTAMP"),
    git_describe: env!("VERGEN_GIT_DESCRIBE"),
};

pub fn print_version(verbose: bool) {
    let version = env!("CARGO_PKG_VERSION");
    if !verbose {
        println!("{}", version);
        return;
    }

    println!("Divvun Runtime v{}", version);
    println!("{:#?}", VERSION_INFO);
}

pub fn print_modules() {
    for module in inventory::iter::<Module>() {
        println!("{}", module);
    }
}

pub struct Bundle {
    _context: Arc<Context>,
    pipe: Pipe,
}

impl Drop for Bundle {
    fn drop(&mut self) {
        tracing::trace!("DROPPING BUNDLE");
    }
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Ast(#[from] ast::Error),
    #[error("{0}")]
    Command(#[from] modules::Error),
    #[error("{0}")]
    Bundle(#[from] OpenError),
}

impl Bundle {
    #[cfg(not(feature = "ffi"))]
    pub fn from_bundle<P: AsRef<Path>>(bundle_path: P) -> Result<Bundle, Error> {
        Self::_from_bundle(bundle_path)
    }

    #[cfg(feature = "ffi")]
    pub(crate) fn from_bundle<P: AsRef<Path>>(bundle_path: P) -> Result<Bundle, Error> {
        Self::_from_bundle(bundle_path)
    }

    fn _from_bundle<P: AsRef<Path>>(bundle_path: P) -> Result<Bundle, Error> {
        tracing::debug!("Loading bundle");
        let temp_dir = tempfile::tempdir()?;
        // log.error("OH WE GO 1");
        let box_file = box_format::BoxFileReader::open(bundle_path)?;
        // log.error("OH WE GO 2");
        let context = Arc::new(Context {
            data: modules::DataRef::BoxFile(Box::new(box_file), temp_dir),
        });

        tracing::debug!("Loading pipeline from context");
        // log.error("OH WE GO 3");
        let defn = context.load_pipeline_definition()?;

        // log.error("OH WE GO 5");
        let pipe = Pipe::new(context.clone(), Arc::new(defn))?;

        tracing::debug!("Returning bundle...");

        // log.error("OH WE GO 6");
        Ok(Bundle {
            _context: context,
            pipe,
        })
    }

    #[cfg(not(feature = "ffi"))]
    pub fn from_path<P: AsRef<Path>>(contents_path: P) -> Result<Bundle, Error> {
        Bundle::_from_path(contents_path)
    }

    fn _from_path<P: AsRef<Path>>(contents_path: P) -> Result<Bundle, Error> {
        // init_py();
        tracing::debug!(
            "Loading bundle from path: {}",
            contents_path.as_ref().display()
        );

        let base = if contents_path.as_ref().is_dir() {
            contents_path.as_ref()
        } else {
            contents_path.as_ref().parent().unwrap()
        };

        let context = Arc::new(Context {
            data: modules::DataRef::Path(base.to_path_buf()),
        });

        tracing::trace!("Loading pipeline definition");
        let defn = context.load_pipeline_definition()?;

        tracing::trace!("Creating pipe");
        let pipe = Pipe::new(context.clone(), Arc::new(defn))?;

        Ok(Bundle {
            _context: context,
            pipe,
        })
    }

    pub async fn create(&self, config: serde_json::Value) -> Result<PipelineHandle, Error> {
        self.pipe
            .create_stream(Arc::new(config), None)
            .await
            .map_err(|e| Error::Ast(e))
    }

    pub async fn create_with_tap(
        &self,
        config: serde_json::Value,
        tap: Arc<dyn Fn(&str, &Command, &InputEvent) + Send + Sync>,
    ) -> Result<PipelineHandle, Error> {
        self.pipe
            .create_stream(Arc::new(config), Some(tap))
            .await
            .map_err(|e| Error::Ast(e))
    }

    pub fn definition(&self) -> &Arc<PipelineDefinition> {
        &self.pipe.defn
    }
}

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct CaughtPanic(String);

#[cfg(feature = "ffi")]
use cffi::{FromForeign, ToForeign, marshal};

#[cfg(feature = "ffi")]
#[no_mangle]
pub fn dr__heartbeat() {
    // use ::oslog::OsLog;

    // let log = ::oslog::OsLog::new("nu.necessary.DivvunExtension", "category");
    // log.error("I AM ALIVE");
}

#[cfg(feature = "ffi")]
#[marshal(return_marshaler = cffi::ArcMarshaler::<Bundle>)]
pub fn dr__bundle__from_bundle(
    #[marshal(cffi::StrMarshaler)] bundle_path: &str,
) -> Result<Arc<Bundle>, Box<dyn std::error::Error>> {
    // let log = ::oslog::OsLog::new("nu.necessary.DivvunExtension", "category");
    // log.error("AWOO AWOO");
    // log.error(&format!("WE IN: {bundle_path:?}"));
    let r = std::panic::catch_unwind(|| match Bundle::_from_bundle(bundle_path) {
        Ok(bundle) => Ok::<_, Error>(bundle),
        Err(e) => {
            // log.error(&format!("{e}"));
            Err(e)
        }
    });

    match r {
        Ok(Ok(v)) => Ok(Arc::new(v)),
        Ok(Err(e)) => Err(Box::new(e) as _),
        Err(e) => Err(Box::new(CaughtPanic(format!("{e:?}"))) as _),
    }
}

#[cfg(feature = "ffi")]
#[marshal]
pub fn dr__bundle__drop(#[marshal(cffi::ArcMarshaler::<Bundle>)] bundle: Arc<Bundle>) {
    drop(bundle);
}

#[cfg(feature = "ffi")]
#[marshal(return_marshaler = BundleArcMarshaler)]
pub fn dr__bundle__from_path(
    #[marshal(cffi::StrMarshaler)] path: &str,
) -> Result<Arc<Bundle>, Box<dyn std::error::Error>> {
    Bundle::_from_path(path)
        .map(Arc::new)
        .map_err(|e| Box::new(e) as _)
}

#[cfg(feature = "ffi")]
type U8VecMarshaler = cffi::VecMarshaler<u8>;
#[cfg(feature = "ffi")]
type BundleArcMarshaler = cffi::ArcMarshaler<Bundle>;
#[cfg(feature = "ffi")]
type BundleArcRefMarshaler = cffi::ArcRefMarshaler<Bundle>;

#[cfg(feature = "ffi")]
thread_local! {
    static RT: tokio::runtime::Runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("Failed building the Runtime");
}

#[cfg(feature = "ffi")]
#[marshal(return_marshaler = U8VecMarshaler)]
pub fn dr__bundle__run_pipeline_bytes(
    #[marshal(BundleArcRefMarshaler)] bundle: Arc<Bundle>,
    #[marshal(cffi::StrMarshaler)] string: &str,
    #[marshal(cffi::StrMarshaler)] config: Option<&str>,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    // let log = ::oslog::OsLog::new("nu.necessary.DivvunExtension", "category");
    // log.error("in run pipeline bytes");
    let s = string.to_string();
    // log.error(&format!("IN: {s}"));

    let config = serde_json::from_str::<serde_json::Value>(config.unwrap_or("{}"))?;

    let r = match RT.with(|rt| {
        // log.error(&format!("RT GET"));
        rt.block_on(bundle._run_pipeline(Input::String(s), Arc::new(config)))
    }) {
        Ok(v) => Ok(v),
        Err(e) => {
            // log.error(&format!("{e}"));
            Err(e)
        }
    }?;

    Ok(r.try_into_bytes()?)
}

#[cfg(feature = "ffi")]
#[marshal(return_marshaler = U8VecMarshaler)]
pub fn dr__bundle__run_pipeline_json(
    #[marshal(BundleArcRefMarshaler)] bundle: Arc<Bundle>,
    #[marshal(cffi::StrMarshaler)] string: &str,
    #[marshal(cffi::StrMarshaler)] config: Option<&str>,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let config = serde_json::from_str::<serde_json::Value>(config.unwrap_or("{}"))?;
    let result = RT.with(|rt| {
        rt.block_on(bundle._run_pipeline(Input::String(string.to_string()), Arc::new(config)))
    })?;
    Ok(serde_json::to_vec(&result.try_into_json()?)?)
}

// #[cfg(feature = "ffi")]
// #[no_mangle]
// pub extern "C" fn dr__debug_repl() {
//     crate::repl::repl();
// }
