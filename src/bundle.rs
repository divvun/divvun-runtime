use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use box_format::OpenError;
use tempfile::TempDir;

use crate::{
    ast::{self, Pipe, PipelineDefinition, PipelineHandle},
    modules::{self, Context, TapFn},
};

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

pub struct Bundle {
    _context: Arc<Context>,
    pipe: Pipe,
}

impl Drop for Bundle {
    fn drop(&mut self) {
        tracing::trace!("DROPPING BUNDLE");
    }
}

impl Bundle {
    pub fn from_bundle<P: AsRef<Path>>(bundle_path: P) -> Result<Bundle, Error> {
        Self::_from_bundle(bundle_path)
    }

    fn _from_bundle<P: AsRef<Path>>(bundle_path: P) -> Result<Bundle, Error> {
        tracing::debug!("Loading bundle");
        let temp_dir = tempfile::tempdir()?;
        let box_file = box_format::BoxFileReader::open(bundle_path)?;
        let context = Arc::new(Context {
            data: modules::DataRef::BoxFile(Box::new(box_file), temp_dir),
        });

        tracing::debug!("Loading pipeline from context");
        let defn = context.load_pipeline_definition()?;
        let pipe = Pipe::new(context.clone(), Arc::new(defn))?;

        tracing::debug!("Returning bundle...");
        Ok(Bundle {
            _context: context,
            pipe,
        })
    }

    pub fn from_path<P: AsRef<Path>>(contents_path: P) -> Result<Bundle, Error> {
        Bundle::_from_path(contents_path)
    }

    fn _from_path<P: AsRef<Path>>(contents_path: P) -> Result<Bundle, Error> {
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
        tap: Arc<TapFn>,
    ) -> Result<PipelineHandle, Error> {
        self.pipe
            .create_stream(Arc::new(config), Some(tap))
            .await
            .map_err(|e| Error::Ast(e))
    }

    pub fn definition(&self) -> &Arc<PipelineDefinition> {
        &self.pipe.defn
    }

    pub fn command<T: modules::CommandRunner>(&self, key: &str) -> Option<&T> {
        self.pipe.command(key)
    }

    pub fn context(&self) -> &Arc<Context> {
        &self._context
    }
}
