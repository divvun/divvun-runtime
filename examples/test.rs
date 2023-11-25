use std::path::Path;

use divvun_runtime::modules::*;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    run().await
}

async fn run() -> anyhow::Result<()> {
    println!(
        "{}",
        pipeline(
            "Soaittášii ahte dát livččii buorre algun ovddid dán forumii viidáseappot.".to_string()
        )
        .await?
    );
    Ok(())
}

async fn pipeline(input: String) -> anyhow::Result<String> {
    let x = hfst::tokenize(Path::new("./tokeniser-gramcheck-gt-desc.pmhfst"), input).await?;
    let x = cg3::vislcg3(Path::new("./valency.bin"), x).await?;
    let x = cg3::vislcg3(Path::new("./mwe-dis.bin"), x).await?;
    let x = cg3::mwesplit(x).await?;
    let x = divvun::blanktag(Path::new("./analyser-gt-whitespace.hfst"), x).await?;
    let x = divvun::cgspell(
        Path::new("./errmodel.default.hfst"),
        Path::new("./acceptor.default.hfst"),
        x,
    )
    .await?;
    let x = cg3::vislcg3(Path::new("./grc-disambiguator.bin"), x).await?;
    let x = cg3::vislcg3(Path::new("./grammarchecker-release.bin"), x).await?;
    let x = divvun::suggest(
        Path::new("./generator-gramcheck-gt-norm.hfstol"),
        Path::new("./errors.xml"),
        x,
    )
    .await?;
    Ok(x)
}
