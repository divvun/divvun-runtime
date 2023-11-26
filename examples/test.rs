use divvun_runtime::load_bundle;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    run().await
}

async fn run() -> anyhow::Result<()> {
    let bundle = load_bundle("./sme-test.drb")?;

    println!(
        "{}",
        bundle
            .run_pipeline(
                "Soaittášii ahte dát livččii buorre algun ovddid dán forumii viidáseappot."
                    .to_string()
            )
            .await?
    );
    Ok(())
}
