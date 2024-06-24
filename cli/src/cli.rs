use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub struct Args {
    #[command(subcommand)]
    pub command: Option<Command>,
    #[clap(short = 'm')]
    pub mods: bool,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    Init(InitArgs),
    Bundle(BundleArgs),
    Sync(SyncArgs),
    Run(RunArgs),
    #[command(flatten)]
    Debug(DebugArgs),
}

#[derive(Subcommand, Debug)]
pub enum DebugArgs {
    DumpAst(DebugDumpAstArgs),
}

#[derive(Parser, Debug)]
pub struct DebugDumpAstArgs {
    #[clap(index = 1)]
    /// Defaults to current directory.
    pub path: PathBuf,
}

#[derive(Parser, Debug)]
pub struct SyncArgs {
    #[clap(index = 1)]
    /// Defaults to current directory.
    pub path: Option<PathBuf>,
}

#[derive(Parser, Debug)]
pub struct InitArgs {
    #[clap(index = 1)]
    /// Defaults to current directory.
    pub path: Option<PathBuf>,
}

#[derive(Parser, Debug)]
pub struct RunArgs {
    #[clap(index = 1)]
    /// Defaults to current directory.
    pub path: PathBuf,

    #[clap(index = 2)]
    pub input: Option<String>,

    #[clap(short, long)]
    pub config: Vec<String>,

    #[clap(short, long)]
    /// Optional path to output data to.
    pub output_path: Option<PathBuf>,

    #[clap(short = 'C', long)]
    /// Run a command against the output path.
    pub command: Option<String>,
}

#[derive(Parser, Debug)]
pub struct BundleArgs {}
