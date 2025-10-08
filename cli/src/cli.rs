use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(author, about, long_about = None)]
pub struct Args {
    #[command(subcommand)]
    pub command: Option<Command>,
    #[clap(short = 'V', long, action = clap::ArgAction::Count)]
    pub version: u8,
    #[clap(short = 'm')]
    pub mods: bool,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    Init(InitArgs),
    Bundle(BundleArgs),
    Sync(SyncArgs),
    Run(RunArgs),
    List(ListArgs),
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
    /// Defaults to current directory.
    #[clap(short, long)]
    pub path: Option<PathBuf>,

    pub input: Option<String>,

    #[clap(short, long)]
    pub config: Vec<String>,

    #[clap(short, long)]
    /// Optional path to output data to.
    pub output_path: Option<PathBuf>,

    #[clap(short = 'C', long)]
    /// Run a command against the output path.
    pub command: Option<String>,

    #[clap(long)]
    /// Skip TypeScript type checking with Deno.
    pub skip_check: bool,

    #[clap(short = 'P', long)]
    /// Select a specific named pipeline from the bundle.
    pub pipeline: Option<String>,
}

#[derive(Parser, Debug)]
pub struct BundleArgs {
    #[clap(short, long)]
    /// Path to the pipeline assets directory.
    pub assets_path: Option<PathBuf>,

    #[clap(short, long)]
    /// Path to the pipeline file.
    pub pipeline_path: Option<PathBuf>,

    #[clap(long)]
    /// Skip TypeScript type checking with Deno.
    pub skip_check: bool,
}

#[derive(Parser, Debug)]
pub struct ListArgs {
    #[clap(index = 1)]
    /// Path to the bundle file or directory containing pipeline.json.
    pub path: Option<PathBuf>,
}
