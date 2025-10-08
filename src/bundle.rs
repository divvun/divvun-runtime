use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use box_format::OpenError;
use tempfile::TempDir;

use crate::{
    ast::{self, Pipe, PipelineBundle, PipelineDefinition, PipelineHandle},
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
    context: Arc<Context>,
    bundle: Arc<PipelineBundle>,
    pipe: Pipe,
}

impl Drop for Bundle {
    fn drop(&mut self) {
        tracing::trace!("DROPPING BUNDLE");
    }
}

impl Bundle {
    pub fn metadata_from_bundle<P: AsRef<Path>>(
        bundle_path: P,
    ) -> Result<Arc<PipelineBundle>, Error> {
        let temp_dir = tempfile::tempdir()?;
        let box_file = box_format::BoxFileReader::open(bundle_path)?;
        let context = Context {
            data: modules::DataRef::BoxFile(Box::new(box_file), temp_dir),
            dev: false,
            base_path: None,
        };
        Ok(Arc::new(context.load_pipeline_bundle()?))
    }

    pub fn metadata_from_path<P: AsRef<Path>>(
        contents_path: P,
    ) -> Result<Arc<PipelineBundle>, Error> {
        let base = if contents_path.as_ref().is_dir() {
            contents_path.as_ref()
        } else {
            contents_path.as_ref().parent().unwrap()
        };

        let context = Context {
            data: modules::DataRef::Path(base.to_path_buf()),
            dev: false,
            base_path: None,
        };
        Ok(Arc::new(context.load_pipeline_bundle()?))
    }

    pub fn from_bundle<P: AsRef<Path>>(bundle_path: P) -> Result<Bundle, Error> {
        Self::_from_bundle(bundle_path)
    }

    fn _from_bundle<P: AsRef<Path>>(bundle_path: P) -> Result<Bundle, Error> {
        Self::_from_bundle_named(bundle_path, None)
    }

    fn _from_bundle_named<P: AsRef<Path>>(
        bundle_path: P,
        pipeline_name: Option<&str>,
    ) -> Result<Bundle, Error> {
        tracing::debug!("Loading bundle");
        let temp_dir = tempfile::tempdir()?;
        let box_file = box_format::BoxFileReader::open(bundle_path)?;
        let mut context = Context {
            data: modules::DataRef::BoxFile(Box::new(box_file), temp_dir),
            dev: false,
            base_path: None,
        };

        tracing::debug!("Loading pipeline bundle from context");
        let bundle = Arc::new(context.load_pipeline_bundle()?);

        tracing::debug!("Loading pipeline definition");
        let defn = if let Some(name) = pipeline_name {
            context.load_pipeline_definition_named(name)?
        } else {
            context.load_pipeline_definition()?
        };

        // Update context with pipeline's dev flag
        context.dev = defn.dev;
        let context = Arc::new(context);

        let pipe = Pipe::new(context.clone(), Arc::new(defn))?;

        tracing::debug!("Returning bundle...");
        Ok(Bundle {
            context,
            bundle,
            pipe,
        })
    }

    pub fn from_bundle_named<P: AsRef<Path>>(
        bundle_path: P,
        pipeline_name: &str,
    ) -> Result<Bundle, Error> {
        Self::_from_bundle_named(bundle_path, Some(pipeline_name))
    }

    pub fn from_path<P: AsRef<Path>>(contents_path: P) -> Result<Bundle, Error> {
        Bundle::_from_path(contents_path)
    }

    fn _from_path<P: AsRef<Path>>(contents_path: P) -> Result<Bundle, Error> {
        Self::_from_path_named(contents_path, None)
    }

    fn _from_path_named<P: AsRef<Path>>(
        contents_path: P,
        pipeline_name: Option<&str>,
    ) -> Result<Bundle, Error> {
        tracing::debug!(
            "Loading bundle from path: {}",
            contents_path.as_ref().display()
        );

        let base = if contents_path.as_ref().is_dir() {
            contents_path.as_ref()
        } else {
            contents_path.as_ref().parent().unwrap()
        };

        let mut context = Context {
            data: modules::DataRef::Path(base.to_path_buf()),
            dev: false,
            base_path: Some(base.to_path_buf()),
        };

        tracing::trace!("Loading pipeline bundle");
        let bundle = Arc::new(context.load_pipeline_bundle()?);

        tracing::trace!("Loading pipeline definition");
        let defn = if let Some(name) = pipeline_name {
            context.load_pipeline_definition_named(name)?
        } else {
            context.load_pipeline_definition()?
        };

        // Update context with pipeline's dev flag
        context.dev = defn.dev;
        let context = Arc::new(context);

        tracing::trace!("Creating pipe");
        let pipe = Pipe::new(context.clone(), Arc::new(defn))?;

        Ok(Bundle {
            context,
            bundle,
            pipe,
        })
    }

    pub fn from_path_named<P: AsRef<Path>>(
        contents_path: P,
        pipeline_name: &str,
    ) -> Result<Bundle, Error> {
        Self::_from_path_named(contents_path, Some(pipeline_name))
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
        &self.context
    }

    pub fn list_pipelines(&self) -> Vec<&str> {
        self.bundle.list_pipelines()
    }

    pub fn bundle(&self) -> &Arc<PipelineBundle> {
        &self.bundle
    }
}
