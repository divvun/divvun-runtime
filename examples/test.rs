use std::sync::Arc;

use divvun_runtime::{Bundle, modules::Context};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    run().await
}

async fn run() -> anyhow::Result<()> {
    let bundle = Bundle::load("./sme-test.drb")?;
    std::env::set_current_dir(bundle.path())?;

    let context: Context = Context {
        path: bundle.path().to_path_buf(),
    };

    println!(
        "{:?}",
        bundle
            .run_pipeline( Arc::new(context),
                "Soaittášii ahte dát livččii buorre algun ovddid dán forumii viidáseappot."
                    .to_string()
            )
            .await?
    );
    Ok(())
}
