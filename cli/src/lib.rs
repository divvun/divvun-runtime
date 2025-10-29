use std::io::IsTerminal;
use std::sync::Arc;

use clap::Parser;
use cli::{Args, Command, DebugArgs};
use command::{
    bundle::bundle,
    init::init,
    list::list,
    playground::playground,
    run::{dump_ast, run},
    sync::sync,
    test::test,
};
use shell::Shell;

mod cli;
mod command;
mod deno_rt;
mod shell;

pub async fn run_cli() -> anyhow::Result<()> {
    let mut shell = Shell::new();

    let args = Args::parse();

    if args.version > 0 {
        divvun_runtime::print_version(args.version > 1);
        std::process::exit(0);
    }

    // Set theme: either from CLI arg, auto-detect, or use default
    let theme = if let Some(theme_name) = args.theme {
        Some(theme_name)
    } else if std::io::stderr().is_terminal() {
        // Auto-detect theme based on terminal background
        match terminal_colorsaurus::theme_mode(terminal_colorsaurus::QueryOptions::default()) {
            Ok(terminal_colorsaurus::ThemeMode::Dark) => {
                Some(syntax_highlight::get_default_theme_for_background(true).to_string())
            }
            Ok(terminal_colorsaurus::ThemeMode::Light) => {
                Some(syntax_highlight::get_default_theme_for_background(false).to_string())
            }
            Err(_) => {
                // Fall back to dark theme if detection fails
                Some(syntax_highlight::get_default_theme_for_background(true).to_string())
            }
        }
    } else {
        None
    };

    shell.set_theme(theme);

    let Some(command) = args.command else {
        eprintln!("No command specified");
        std::process::exit(1);
    };

    match command {
        Command::Init(args) => init(&mut shell, args).await?,
        Command::Run(args) => run(&mut shell, args)
            .await
            .map_err(|e| Arc::try_unwrap(e).unwrap())?,
        Command::Sync(args) => sync(&mut shell, args).await?,
        Command::Bundle(args) => bundle(&mut shell, args)?,
        Command::List(args) => list(&mut shell, args)?,
        Command::Playground(args) => playground(&mut shell, args)?,
        Command::Test(args) => test(&mut shell, args).await?,
        Command::Debug(args) => match args {
            DebugArgs::DumpAst(args) => {
                dump_ast(&mut shell, args)?;
            }
        },
    }

    Ok(())
}
