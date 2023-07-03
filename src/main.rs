#[tokio::main]
async fn main() -> anyhow::Result<()> {
    divvun_runtime::run().await
}
