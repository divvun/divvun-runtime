#![allow(non_snake_case)]

use std::sync::Arc;

use cffi::{FromForeign, ToForeign, marshal};
use futures_util::StreamExt;

use crate::{
    ast::PipelineHandle, bundle::{self, Bundle}, modules::Input
};

type U8VecMarshaler = cffi::VecMarshaler<u8>;
type BundleArcMarshaler = cffi::ArcMarshaler<Bundle>;
type BundleArcRefMarshaler = cffi::ArcRefMarshaler<Bundle>;
type PipelineHandleBoxMarshaler = cffi::BoxMarshaler<PipelineHandle>;
type PipelineHandleBoxMutRefMarshaler = cffi::BoxMutRefMarshaler<PipelineHandle>;

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct CaughtPanic(String);

thread_local! {
    static RT: tokio::runtime::Runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("Failed building the Runtime");
}

#[marshal(return_marshaler = cffi::ArcMarshaler::<Bundle>)]
pub fn DRT_Bundle_fromBundle(
    #[marshal(cffi::StrMarshaler)] bundle_path: &str,
) -> Result<Arc<Bundle>, Box<dyn std::error::Error>> {
    let r = std::panic::catch_unwind(|| match Bundle::from_bundle(bundle_path) {
        Ok(bundle) => Ok::<_, bundle::Error>(bundle),
        Err(e) => Err(e),
    });

    match r {
        Ok(Ok(v)) => Ok(Arc::new(v)),
        Ok(Err(e)) => Err(Box::new(e) as _),
        Err(e) => Err(Box::new(CaughtPanic(format!("{e:?}"))) as _),
    }
}

#[marshal]
pub fn DRT_Bundle_drop(#[marshal(cffi::ArcMarshaler::<Bundle>)] bundle: Arc<Bundle>) {
    drop(bundle);
}

#[marshal(return_marshaler = BundleArcMarshaler)]
pub fn DRT_Bundle_fromPath(
    #[marshal(cffi::StrMarshaler)] path: &str,
) -> Result<Arc<Bundle>, Box<dyn std::error::Error>> {
    Bundle::from_path(path)
        .map(Arc::new)
        .map_err(|e| Box::new(e) as _)
}

#[marshal(return_marshaler = PipelineHandleBoxMarshaler)]
pub fn DRT_Bundle_create(
    #[marshal(BundleArcRefMarshaler)] bundle: Arc<Bundle>,
    #[marshal(cffi::StrMarshaler)] config: Option<&str>,
) -> Result<Box<PipelineHandle>, Box<dyn std::error::Error>> {
    let config = serde_json::from_str::<serde_json::Value>(config.unwrap_or("{}"))?;

    Ok(RT.with(|rt| {
        rt.block_on(async move {
            bundle.create(config).await
        })
    }).map(Box::new)?)
}

#[marshal]
pub fn DRT_PipelineHandle_drop(#[marshal(PipelineHandleBoxMarshaler)] handle: Box<PipelineHandle>) {
    drop(handle);
}

#[marshal]
pub fn DRT_Vec_drop(#[marshal(U8VecMarshaler)] vec: Vec<u8>) {
    drop(vec);
}

#[marshal(return_marshaler = U8VecMarshaler)]
pub fn DRT_PipelineHandle_forward(
    #[marshal(PipelineHandleBoxMutRefMarshaler)] pipe: &mut PipelineHandle,
    #[marshal(cffi::StrMarshaler)] input: &str,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let s = input.to_string();
    
    Ok(RT.with(|rt| {
        rt.block_on(async move {
            let mut stream = pipe.forward(Input::String(s)).await;

            while let Some(Ok(input)) = stream.next().await {
                match input {
                    Input::Bytes(items) => return Ok(items),
                    Input::String(s) => return Ok(s.into_bytes()),
                    Input::Json(v) => {
                        return Ok(serde_json::to_vec(&v).map_err(|e| {
                            crate::bundle::Error::Io(std::io::Error::new(
                                std::io::ErrorKind::Other,
                                e.to_string(),
                            ))
                        })?);
                    }
                    Input::ArrayBytes(_) => {
                        return Err(crate::bundle::Error::Io(std::io::Error::new(
                            std::io::ErrorKind::Other,
                            "Expected array of bytes output from pipeline",
                        )));
                    }
                    Input::ArrayString(_) => {
                        return Err(crate::bundle::Error::Io(std::io::Error::new(
                            std::io::ErrorKind::Other,
                            "Expected array of strings output from pipeline",
                        )));
                    }
                    Input::Multiple(_) => {
                        return Err(crate::bundle::Error::Io(std::io::Error::new(
                            std::io::ErrorKind::Other,
                            "Expected multiple outputs from pipeline",
                        )));
                    }
                }
            }

            Err(crate::bundle::Error::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                "Pipeline produced no output",
            )))
        })
    })?)
}

#[marshal(return_marshaler = U8VecMarshaler)]
pub fn DRT_Bundle_runPipeline(
    #[marshal(BundleArcRefMarshaler)] bundle: Arc<Bundle>,
    #[marshal(cffi::StrMarshaler)] string: &str,
    #[marshal(cffi::StrMarshaler)] config: Option<&str>,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let s = string.to_string();
    let config = serde_json::from_str::<serde_json::Value>(config.unwrap_or("{}"))?;

    Ok(RT.with(|rt| {
        rt.block_on(async move {
            let mut pipe = bundle.create(config).await?;
            let mut stream = pipe.forward(Input::String(s)).await;

            while let Some(Ok(input)) = stream.next().await {
                match input {
                    Input::Bytes(items) => return Ok(items),
                    Input::String(s) => return Ok(s.into_bytes()),
                    Input::Json(v) => {
                        return Ok(serde_json::to_vec(&v).map_err(|e| {
                            crate::bundle::Error::Io(std::io::Error::new(
                                std::io::ErrorKind::Other,
                                e.to_string(),
                            ))
                        })?);
                    }
                    Input::ArrayBytes(_) => {
                        return Err(crate::bundle::Error::Io(std::io::Error::new(
                            std::io::ErrorKind::Other,
                            "Expected array of bytes output from pipeline",
                        )));
                    }
                    Input::ArrayString(_) => {
                        return Err(crate::bundle::Error::Io(std::io::Error::new(
                            std::io::ErrorKind::Other,
                            "Expected array of strings output from pipeline",
                        )));
                    }
                    Input::Multiple(_) => {
                        return Err(crate::bundle::Error::Io(std::io::Error::new(
                            std::io::ErrorKind::Other,
                            "Expected multiple outputs from pipeline",
                        )));
                    }
                }
            }

            Err(crate::bundle::Error::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                "Pipeline produced no output",
            )))
        })
    })?)
}
