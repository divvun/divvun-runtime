#![allow(non_snake_case)]

use std::sync::Arc;

use cffi::{FromForeign, ToForeign, marshal};
use futures_util::StreamExt;

use crate::{ast::PipelineHandle, bundle::Bundle, modules::PipelineValue};

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
    let bundle_path = bundle_path.to_string();
    RT.with(|rt| {
        rt.block_on(async move {
            Bundle::from_bundle(&bundle_path)
                .await
                .map(Arc::new)
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)
        })
    })
}

const _: () = {
    std::hint::black_box(DRT_Bundle_fromBundle);
};

#[marshal]
pub fn DRT_Bundle_drop(#[marshal(cffi::ArcMarshaler::<Bundle>)] bundle: Arc<Bundle>) {
    drop(bundle);
}

#[marshal(return_marshaler = BundleArcMarshaler)]
pub fn DRT_Bundle_fromPath(
    #[marshal(cffi::StrMarshaler)] path: &str,
) -> Result<Arc<Bundle>, Box<dyn std::error::Error>> {
    let path = path.to_string();
    RT.with(|rt| {
        rt.block_on(async move {
            Bundle::from_path(&path)
                .await
                .map(Arc::new)
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)
        })
    })
}

#[marshal(return_marshaler = PipelineHandleBoxMarshaler)]
pub fn DRT_Bundle_create(
    #[marshal(BundleArcRefMarshaler)] bundle: Arc<Bundle>,
    #[marshal(cffi::StrMarshaler)] config: Option<&str>,
) -> Result<Box<PipelineHandle>, Box<dyn std::error::Error>> {
    let config = serde_json::from_str::<serde_json::Value>(config.unwrap_or("{}"))?;

    Ok(RT
        .with(|rt| rt.block_on(async move { bundle.create(config).await }))
        .map(Box::new)?)
}

#[marshal]
pub fn DRT_PipelineHandle_drop(#[marshal(PipelineHandleBoxMarshaler)] handle: Box<PipelineHandle>) {
    drop(handle);
}

#[marshal]
pub fn DRT_PipelineHandle_cancel(
    #[marshal(PipelineHandleBoxMutRefMarshaler)] pipe: &mut PipelineHandle,
) {
    RT.with(|rt| rt.block_on(pipe.cancel()));
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
            let mut stream = pipe.forward(PipelineValue::String(s)).await;

            while let Some(Ok(input)) = stream.next().await {
                match input {
                    PipelineValue::Bytes(items) => return Ok(items),
                    PipelineValue::String(s) => return Ok(s.into_bytes()),
                    PipelineValue::Json(v) => {
                        return Ok(serde_json::to_vec(&v).map_err(|e| {
                            crate::bundle::Error::Io(std::io::Error::new(
                                std::io::ErrorKind::Other,
                                e.to_string(),
                            ))
                        })?);
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
            let mut stream = pipe.forward(PipelineValue::String(s)).await;

            while let Some(Ok(input)) = stream.next().await {
                match input {
                    PipelineValue::Bytes(items) => return Ok(items),
                    PipelineValue::String(s) => return Ok(s.into_bytes()),
                    PipelineValue::Json(v) => {
                        return Ok(serde_json::to_vec(&v).map_err(|e| {
                            crate::bundle::Error::Io(std::io::Error::new(
                                std::io::ErrorKind::Other,
                                e.to_string(),
                            ))
                        })?);
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
pub fn DRT_Bundle_errorPreferences(
    #[marshal(BundleArcRefMarshaler)] bundle: Arc<Bundle>,
    #[marshal(cffi::StrMarshaler)] locales_json: &str,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let locales: Vec<String> = serde_json::from_str(locales_json)?;
    let locale_refs: Vec<&str> = locales.iter().map(|s| s.as_str()).collect();
    let Some((_, suggest)) = bundle.command::<crate::modules::divvun::Suggest>(None) else {
        return Err("Suggest command not found in bundle".into());
    };
    let prefs = suggest.error_preferences(&locale_refs);
    Ok(serde_json::to_vec(&prefs)?)
}
