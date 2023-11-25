use std::path::{Path, PathBuf};

use divvun_runtime::{modules::*, ast::{from_ast, PipelineDefinition}};

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
    let jd = &mut serde_json::Deserializer::from_str(include_str!("./ast.json"));
    let defn: PipelineDefinition = serde_path_to_error::deserialize(jd)?;
    
    // let defn: PipelineDefinition = serde_json::from_str().unwrap();
    let result = from_ast(defn.ast, Box::pin(async {
        Ok(input)
    }))?.await?;
    Ok(result)
    // let x = Box::pin(hfst::tokenize(
    //     PathBuf::from("./tokeniser-gramcheck-gt-desc.pmhfst"),
    //     Box::pin(async move { Ok(input) }),
    // ));
    // let x = Box::pin(cg3::vislcg3(PathBuf::from("./valency.bin"), x));
    // let x = Box::pin(cg3::vislcg3(PathBuf::from("./mwe-dis.bin"), x));
    // let x = Box::pin(cg3::mwesplit(x));
    // let x = Box::pin(divvun::blanktag(
    //     PathBuf::from("./analyser-gt-whitespace.hfst"),
    //     x,
    // ));
    // let x = Box::pin(divvun::cgspell(
    //     PathBuf::from("./errmodel.default.hfst"),
    //     PathBuf::from("./acceptor.default.hfst"),
    //     x,
    // ));
    // let x = Box::pin(cg3::vislcg3(PathBuf::from("./grc-disambiguator.bin"), x));
    // let x = Box::pin(cg3::vislcg3(
    //     PathBuf::from("./grammarchecker-release.bin"),
    //     x,
    // ));
    // let x = Box::pin(divvun::suggest(
    //     PathBuf::from("./generator-gramcheck-gt-norm.hfstol"),
    //     PathBuf::from("./errors.xml"),
    //     x,
    // ));
    // Ok(x.await?)
}
