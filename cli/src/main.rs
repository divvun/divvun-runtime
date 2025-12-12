fn main() -> miette::Result<()> {
    miette::set_hook(Box::new(|_| {
        Box::new(
            miette::MietteHandlerOpts::new()
                .terminal_links(false)
                .context_lines(0)
                .build(),
        )
    }))?;

    tracing_subscriber::fmt::init();

    tokio::runtime::Runtime::new()
        .map_err(|e| miette::miette!("Failed to create tokio runtime: {}", e))?
        .block_on(divvun_runtime_cli::run_cli())
}
