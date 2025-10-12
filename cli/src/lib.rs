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
