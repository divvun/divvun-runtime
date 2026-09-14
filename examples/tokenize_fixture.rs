//! Run a pipeline directory over a few inputs and print each result, so the
//! same fixture can be replayed across a library change and the two outputs
//! diffed.
//!
//! Usage: cargo run --release --example tokenize_fixture -- <pipeline-dir>

use divvun_runtime::modules::PipelineValue;
use divvun_runtime::bundle::Bundle;
use futures_util::StreamExt;

const INPUTS: &[&str] = &[
    "Mun lean Márjá ja mun ásan Guovdageainnus.",
    "Dat lea 3. beaivi, ja son bođii diibmu 14.30 áigge.",
    "Sámediggi mearridii ahte ođđa láhka boahtá fápmui 2026:s.",
    "Boahtte vahkku mii vuolgit Romsii — jos dálki lea buorre!",
    "«Mii dál dáhpáhuvvá?» jearai son.",
];

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = std::env::args()
        .nth(1)
        .ok_or("usage: tokenize_fixture <pipeline-dir>")?;

    let bundle = Bundle::from_path(&dir).await?;
    let mut pipe = bundle.create(serde_json::json!({})).await?;

    for input in INPUTS {
        println!("=== {input}");
        let mut stream = pipe.forward(PipelineValue::String(input.to_string())).await;
        while let Some(item) = stream.next().await {
            match item? {
                PipelineValue::String(s) => println!("{s}"),
                other => println!("{other:?}"),
            }
        }
    }

    Ok(())
}
