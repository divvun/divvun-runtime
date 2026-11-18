#[cfg(feature = "fs")]
use std::path::Path;
use std::sync::Arc;

#[cfg(feature = "fs")]
use box_format::OpenError;

use crate::{
    ast::{self, Pipe, PipelineBundle, PipelineDefinition, PipelineHandle},
    modules::{self, Context, MemoryBundle, TapFn},
};

#[derive(Debug, thiserror::Error, miette::Diagnostic)]
pub enum Error {
    // See `ast::Error`: `transparent` keeps the message from being repeated
    // once per wrapper layer.
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    #[diagnostic(transparent)]
    Ast(#[from] ast::Error),
    #[error(transparent)]
    #[diagnostic(transparent)]
    Command(#[from] modules::Error),
    #[cfg(feature = "fs")]
    #[error(transparent)]
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
    /// The pipelines of the bundle archive `bytes` (a `.drb`), without
    /// loading any of them.
    pub async fn metadata_from_bytes(bytes: Vec<u8>) -> Result<Arc<PipelineBundle>, Error> {
        let context = Context {
            data: modules::DataRef::Memory(MemoryBundle::new(bytes)?),
            dev: false,
            base_path: None,
        };
        Ok(Arc::new(context.load_pipeline_bundle().await?))
    }

    /// Load the default pipeline of the bundle archive `bytes` (a `.drb`),
    /// held in memory: how a build without the `fs` feature reads a bundle.
    pub async fn from_bytes(bytes: Vec<u8>) -> Result<Bundle, Error> {
        Self::_from_bytes_named(bytes, None).await
    }

    pub async fn from_bytes_named(bytes: Vec<u8>, pipeline_name: &str) -> Result<Bundle, Error> {
        Self::_from_bytes_named(bytes, Some(pipeline_name)).await
    }

    async fn _from_bytes_named(
        bytes: Vec<u8>,
        pipeline_name: Option<&str>,
    ) -> Result<Bundle, Error> {
        let context = Context {
            data: modules::DataRef::Memory(MemoryBundle::new(bytes)?),
            dev: false,
            base_path: None,
        };
        Self::from_context(context, pipeline_name).await
    }

    async fn from_context(
        mut context: Context,
        pipeline_name: Option<&str>,
    ) -> Result<Bundle, Error> {
        let bundle = Arc::new(context.load_pipeline_bundle().await?);
        let defn = if let Some(name) = pipeline_name {
            context.load_pipeline_definition_named(name).await?
        } else {
            context.load_pipeline_definition().await?
        };

        // Update context with pipeline's dev flag
        context.dev = defn.dev;
        let context = Arc::new(context);

        let pipe = Pipe::new(context.clone(), Arc::new(defn)).await?;

        Ok(Bundle {
            context,
            bundle,
            pipe,
        })
    }

    #[cfg(feature = "fs")]
    pub async fn metadata_from_bundle<P: AsRef<Path>>(
        bundle_path: P,
    ) -> Result<Arc<PipelineBundle>, Error> {
        let box_file = box_format::BoxFileReader::open(bundle_path).await?;
        let context = Context {
            data: modules::DataRef::BoxFile(Box::new(box_file)),
            dev: false,
            base_path: None,
        };
        Ok(Arc::new(context.load_pipeline_bundle().await?))
    }

    #[cfg(feature = "fs")]
    pub async fn metadata_from_path<P: AsRef<Path>>(
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
        Ok(Arc::new(context.load_pipeline_bundle().await?))
    }

    #[cfg(feature = "fs")]
    pub async fn from_bundle<P: AsRef<Path>>(bundle_path: P) -> Result<Bundle, Error> {
        Self::_from_bundle_named(bundle_path, None).await
    }

    #[cfg(feature = "fs")]
    async fn _from_bundle_named<P: AsRef<Path>>(
        bundle_path: P,
        pipeline_name: Option<&str>,
    ) -> Result<Bundle, Error> {
        tracing::debug!("Loading bundle");
        let box_file = box_format::BoxFileReader::open(bundle_path).await?;
        let context = Context {
            data: modules::DataRef::BoxFile(Box::new(box_file)),
            dev: false,
            base_path: None,
        };
        Self::from_context(context, pipeline_name).await
    }

    #[cfg(feature = "fs")]
    pub async fn from_bundle_named<P: AsRef<Path>>(
        bundle_path: P,
        pipeline_name: &str,
    ) -> Result<Bundle, Error> {
        Self::_from_bundle_named(bundle_path, Some(pipeline_name)).await
    }

    #[cfg(feature = "fs")]
    pub async fn from_path<P: AsRef<Path>>(contents_path: P) -> Result<Bundle, Error> {
        Self::_from_path_named(contents_path, None).await
    }

    #[cfg(feature = "fs")]
    async fn _from_path_named<P: AsRef<Path>>(
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

        let context = Context {
            data: modules::DataRef::Path(base.to_path_buf()),
            dev: false,
            base_path: Some(base.to_path_buf()),
        };
        Self::from_context(context, pipeline_name).await
    }

    #[cfg(feature = "fs")]
    pub async fn from_path_named<P: AsRef<Path>>(
        contents_path: P,
        pipeline_name: &str,
    ) -> Result<Bundle, Error> {
        Self::_from_path_named(contents_path, Some(pipeline_name)).await
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

    pub fn command<'a, T: modules::CommandRunner>(
        &'a self,
        key: Option<&'a str>,
    ) -> Option<(&'a str, &'a T)> {
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
