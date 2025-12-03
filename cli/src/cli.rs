use std::{path::PathBuf, str};

use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(author, about, long_about = None)]
pub struct Args {
    #[command(subcommand)]
    pub command: Option<Command>,
    /// Show version information. Use -VV for more details.
    #[clap(short = 'V', long, action = clap::ArgAction::Count)]
    pub version: u8,
    /// Select syntax highlighting theme. Available themes:
    ///   Dark: base16-ocean.dark, base16-eighties.dark, base16-mocha.dark, Solarized (dark)
    ///   Light: base16-ocean.light, InspiredGitHub, Solarized (light)
    ///   Default: auto-detect based on terminal background
    #[clap(long, env = "DRT_THEME")]
    pub theme: Option<String>,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Initialize a new bundle project with example TypeScript pipeline
    Init(InitArgs),
    /// Package a TypeScript pipeline and assets into a .drb bundle file
    Bundle(BundleArgs),
    /// Sync TypeScript bindings and runtime environment
    Sync(SyncArgs),
    /// Execute a pipeline bundle (starts REPL if no input provided)
    Run(RunArgs),
    /// List available pipelines and metadata from a bundle
    List(ListArgs),
    /// Open a bundle in the graphical playground/debugger
    #[command(alias = "play")]
    Playground(PlaygroundArgs),
    /// Run TypeScript test files using Deno
    Test(TestArgs),
    /// Run YAML-defined tests against a pipeline bundle
    YamlTest(YamlTestArgs),
    #[command(flatten)]
    Debug(DebugArgs),
}

#[derive(Subcommand, Debug)]
pub enum DebugArgs {
    /// Dump the parsed pipeline AST as JSON
    #[command(hide = true)]
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

    #[clap(long)]
    /// Bundle type metadata (e.g., "grammar", "spellcheck", "tts").
    pub r#type: Option<String>,

    #[clap(long)]
    /// Bundle name metadata.
    pub name: Option<String>,

    #[clap(long)]
    /// Bundle version metadata.
    pub vers: Option<String>,
}

#[derive(Parser, Debug)]
pub struct ListArgs {
    #[clap(index = 1)]
    /// Path to the bundle file or directory containing pipeline.json.
    pub path: Option<PathBuf>,
}

#[derive(Parser, Debug)]
pub struct PlaygroundArgs {
    #[clap(index = 1)]
    /// Path to open in playground. Defaults to current directory.
    pub path: Option<PathBuf>,
}

#[derive(Parser, Debug)]
pub struct TestArgs {
    /// Test files to run
    pub files: Vec<PathBuf>,

    /// Arguments to pass to the test script (after --)
    #[clap(last = true)]
    pub script_args: Vec<String>,
}

#[derive(Parser, Debug)]
pub struct YamlTestArgs {
    /// Path to the YAML test definition file
    pub yaml_file: PathBuf,
    
    /// Path to the bundle directory or .ts file. Defaults to current directory.
    #[clap(short, long)]
    pub path: Option<PathBuf>,
    
    /// Select a specific named pipeline from the bundle.
    #[clap(short = 'P', long)]
    pub pipeline: Option<String>,
}