use divvun_runtime::ast::{from_ast, PipelineDefinition};

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
    let result = from_ast(defn.ast, Box::pin(async { Ok(input) }))?.await?;
    Ok(result)
}
