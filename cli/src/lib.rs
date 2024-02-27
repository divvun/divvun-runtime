use std::sync::Arc;

use clap::Parser;
use cli::{Args, Command, DebugArgs};
use command::{
    bundle::bundle,
    init::init,
    run::{dump_ast, run},
    sync::sync,
};
use shell::Shell;

mod cli;
mod command;
mod shell;

pub async fn run_cli() -> anyhow::Result<()> {
    let mut shell = Shell::new();

    if std::env::args().skip(1).next() == Some("py".to_string()) {
        divvun_runtime::repl::repl();
        return Ok(());
    }

    let args = Args::parse();

    if args.mods {
        divvun_runtime::print_modules();
        std::process::exit(0);
    }

    let Some(command) = args.command else {
        eprintln!("No command specified");
        std::process::exit(1);
    };

    match command {
        Command::Init(args) => init(&mut shell, args).await?,
        Command::Run(args) => run(&mut shell, args).await.unwrap(),
        Command::Sync(args) => sync(&mut shell, args).await?,
        Command::Bundle(args) => bundle(&mut shell, args)?,
        Command::Debug(args) => match args {
            DebugArgs::DumpAst(args) => {
                dump_ast(&mut shell, args)?;
            }
        },
    }

    Ok(())
}
