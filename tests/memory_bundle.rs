//! A bundle read from its bytes in memory, the way a build without the `fs`
//! feature (wasm32-unknown-unknown) reads every bundle.

#![cfg(feature = "mod-cg3")]

use box_format::{BoxFileWriter, BoxPath, Compression, CompressionConfig};
use divvun_runtime::{bundle::Bundle, modules::PipelineValue};
use futures_util::StreamExt;
use serde_json::json;

const GRAMMAR: &str = "DELIMITERS = \"<.>\" ;\nLIST N = N ;\nSELECT N ;\n";
const INPUT: &str = "\"<viessu>\"\n\t\"viessu\" N\n\t\"viessu\" V\n";

/// A `.drb` holding `files`, written as `divvun-runtime bundle` writes one.
async fn bundle_bytes(files: &[(&str, &[u8])]) -> Vec<u8> {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("bundle.drb");
    let mut writer = BoxFileWriter::create_with_alignment(&path, 16)
        .await
        .unwrap();
    for (name, bytes) in files {
        writer
            .insert(
                &CompressionConfig::new(Compression::Stored),
                BoxPath::new(name).unwrap(),
                &mut std::io::Cursor::new(bytes.to_vec()),
                Default::default(),
            )
            .await
            .unwrap();
    }
    writer.finish().await.unwrap();
    std::fs::read(&path).unwrap()
}

fn pipeline(grammar_path: &str) -> Vec<u8> {
    serde_json::to_vec(&json!({
        "version": 1,
        "default": "disambiguate",
        "pipelines": {
            "disambiguate": {
                "entry": { "value_type": "string" },
                "output": { "ref": "cg" },
                "commands": {
                    "cg": {
                        "module": "cg3",
                        "command": "vislcg3",
                        "args": { "model_path": { "type": "path", "value": grammar_path } },
                        "input": { "ref": "#/entry" },
                        "returns": "string"
                    }
                }
            }
        }
    }))
    .unwrap()
}

#[tokio::test]
async fn a_pipeline_runs_from_a_bundle_in_memory() {
    let bytes = bundle_bytes(&[
        ("pipeline.json", &pipeline("grammar.cg3")),
        ("grammar.cg3", GRAMMAR.as_bytes()),
    ])
    .await;

    let bundle = Bundle::from_bytes(bytes).await.expect("the bundle loads");
    assert_eq!(bundle.list_pipelines(), ["disambiguate"]);

    let mut pipe = bundle.create(json!({})).await.expect("a pipeline");
    let mut output = pipe.forward(PipelineValue::String(INPUT.into())).await;
    let mut text = String::new();
    while let Some(value) = output.next().await {
        if let PipelineValue::String(s) = value.expect("output") {
            text.push_str(&s);
        }
    }

    assert!(text.contains("\"viessu\" N"), "got: {text}");
    assert!(!text.contains("\"viessu\" V"), "got: {text}");
}

#[tokio::test]
async fn an_asset_missing_from_the_bundle_is_named() {
    let bytes = bundle_bytes(&[("pipeline.json", &pipeline("absent.cg3"))]).await;

    let Err(error) = Bundle::from_bytes(bytes).await else {
        panic!("a pipeline built without its grammar");
    };
    let report = format!("{:?}", miette::Report::new(error));
    assert!(report.contains("not found in the bundle"), "got: {report}");
    assert!(report.contains("absent.cg3"), "got: {report}");
}
