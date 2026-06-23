use std::io::IsTerminal;

fn main() -> miette::Result<()> {
    miette::set_hook(Box::new(|_| {
        Box::new(
            miette::MietteHandlerOpts::new()
                .terminal_links(false)
                .context_lines(0)
                .build(),
        )
    }))?;

    // Diagnostic logs go to stderr (not stdout) so they never pollute piped
    // output, and only use ANSI colour when stderr is a terminal (#39).
    // RUST_LOG keeps overriding the default `info` level as before.
    let filter = std::env::var("RUST_LOG")
        .map(tracing_subscriber::EnvFilter::new)
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .with_ansi(std::io::stderr().is_terminal())
        .init();

    tokio::runtime::Runtime::new()
        .map_err(|e| miette::miette!("Failed to create tokio runtime: {}", e))?
        .block_on(divvun_runtime_cli::run_cli())
}
