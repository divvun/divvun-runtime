use divvun_runtime::Bundle;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    run().await
}

async fn run() -> anyhow::Result<()> {
    let bundle = Bundle::load("./sme-test.drb")?;
    std::env::set_current_dir(bundle.path())?;

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
