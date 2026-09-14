//! Open the same pipeline directory as N bundles and print resident memory
//! after each, so cross-bundle sharing of the tokenizer core is measurable.
//!
//! Usage: cargo run --release --example core_cache_probe -- <pipeline-dir> [n]

use divvun_runtime::bundle::Bundle;
use divvun_runtime::modules::PipelineValue;
use futures_util::StreamExt;

fn rss_mib() -> f64 {
    let out = std::process::Command::new("/bin/ps")
        .args(["-o", "rss=", "-p", &std::process::id().to_string()])
        .output()
        .expect("ps runs");
    String::from_utf8_lossy(&out.stdout)
        .trim()
        .parse::<f64>()
        .unwrap_or(0.0)
        / 1024.0
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();
    let dir = std::env::args()
        .nth(1)
        .ok_or("usage: core_cache_probe <pipeline-dir> [n]")?;
    let n: usize = std::env::args()
        .nth(2)
        .and_then(|s| s.parse().ok())
        .unwrap_or(2);

    let mut bundles = Vec::new();
    println!("start: {:.1} MiB", rss_mib());
    for i in 0..n {
        let t = std::time::Instant::now();
        let bundle = Bundle::from_path(&dir).await?;
        let pipe = bundle.create(serde_json::json!({})).await?;
        println!(
            "bundle {}: {:.1} MiB ({} ms)",
            i + 1,
            rss_mib(),
            t.elapsed().as_millis()
        );
        bundles.push((bundle, pipe));
    }

    // One sentence through the last pipeline proves the shared core tokenizes.
    let (_, pipe) = bundles.last_mut().ok_or("no bundles")?;
    let mut stream = pipe
        .forward(PipelineValue::String("Mun lean Márjá.".to_string()))
        .await;
    let mut lines = 0usize;
    while let Some(item) = stream.next().await {
        if let PipelineValue::String(s) = item? {
            lines += s.lines().count();
        }
    }
    println!("tokenised lines: {lines}");
    Ok(())
}
