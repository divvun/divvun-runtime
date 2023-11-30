use std::sync::Arc;

use divvun_runtime::{modules::{Context, speech::CELL}, Bundle};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    run().await
}

async fn run() -> anyhow::Result<()> {
    let bundle = Bundle::load("./tts-sme.drb")?;
    // std::env::set_current_dir(bundle.path())?;

    let context = Arc::new(Context {
        path: bundle.path().to_path_buf(),
    });

    let output = bundle
        .run_pipeline(
            context.clone(),
            "Soaittášii ahte dát livččii buorre algun ovddid dán forumii viidáseappot."
                .to_string()
                .into(),
        )
        .await?
        .try_into_bytes()
        .unwrap();

    println!("{:?}", output.len());


    let output = bundle
        .run_pipeline(
            context.clone(),
            "Soaittášii ahte dát livččii buorre algun ovddid dán forumii viidáseappot."
                .to_string()
                .into(),
        )
        .await?
        .try_into_bytes()
        .unwrap();
    println!("{:?}", output.len());

    println!("Killing");
    let (tx, _, _) = CELL.get().unwrap();
    tx.send(None).await?;
    println!("Dying");

    Ok(())
}
