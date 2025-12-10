use anyhow::Result;
use clap::{ArgAction, Parser, Subcommand, ValueEnum};
use std::fs;
use std::io::{self, Write};
use std::path::Path;
use xml_conv::{fluent, kdl, mangle, parse_xml_to_errors, validate};

#[derive(ValueEnum, Clone, Debug)]
enum OutputFormat {
    Json,
    Fluent,
    Kdl,
}

#[derive(Parser)]
#[command(name = "xml-conv")]
#[command(about = "Convert XML files and validate/mangle Fluent files")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Convert XML to JSON, Fluent, or KDL format
    Convert {
        /// Input XML file path
        #[arg(short, long)]
        input: String,

        /// Output format
        #[arg(short, long, value_enum, default_value = "json")]
        format: OutputFormat,

        /// Output file path (for JSON) or directory (for Fluent)
        #[arg(short, long)]
        output: Option<String>,

        /// Pretty-print the JSON output (ignored for Fluent)
        #[arg(short, long, action = ArgAction::SetTrue)]
        pretty: bool,
    },

    /// Validate FTL files against errors.json
    Validate {
        /// Path to errors.json
        #[arg(long)]
        json: String,

        /// Path(s) to FTL files
        #[arg(long, required = true)]
        ftl: Vec<String>,
    },

    /// Mangle unicode IDs in FTL files to encoded form
    Mangle {
        /// Path(s) to FTL files to process
        ftl: Vec<String>,

        /// Write changes in-place (otherwise dry-run)
        #[arg(long, action = ArgAction::SetTrue)]
        write: bool,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Convert {
            input,
            format,
            output,
            pretty,
        } => {
            let xml_content = fs::read_to_string(&input)?;
            let error_doc = parse_xml_to_errors(&xml_content)?;

            match format {
                OutputFormat::Json => {
                    let json_output = if pretty {
                        serde_json::to_string_pretty(&error_doc)?
                    } else {
                        serde_json::to_string(&error_doc)?
                    };

                    match output {
                        Some(output_path) => {
                            fs::write(&output_path, json_output)?;
                            println!("Converted {} to {}", input, output_path);

                            // Generate errors.json metadata file alongside JSON output
                            let output_dir =
                                Path::new(&output_path).parent().unwrap_or(Path::new("."));
                            let errors_metadata = fluent::generate_errors_metadata(&error_doc)?;
                            let errors_json_path = output_dir.join("errors.json");
                            fs::write(&errors_json_path, errors_metadata)?;
                            println!("Generated metadata file: {}", errors_json_path.display());
                        }
                        None => {
                            print!("{}", json_output);
                            io::stdout().flush()?;
                        }
                    }
                }
                OutputFormat::Fluent => {
                    let output_dir = output.as_deref().unwrap_or(".");
                    let output_path = Path::new(output_dir);

                    fluent::write_fluent_files(&error_doc, output_path)?;
                    println!("Converted {} to Fluent files in {}", input, output_dir);
                }
                OutputFormat::Kdl => {
                    let kdl_output = kdl::to_kdl(&error_doc)?;

                    match output {
                        Some(output_path) => {
                            fs::write(&output_path, &kdl_output)?;
                            println!("Converted {} to {}", input, output_path);
                        }
                        None => {
                            print!("{}", kdl_output);
                            io::stdout().flush()?;
                        }
                    }
                }
            }
        }

        Commands::Validate { json, ftl } => {
            let json_path = Path::new(&json);
            let ftl_paths: Vec<_> = ftl.iter().map(|p| Path::new(p)).collect();
            let ftl_refs: Vec<_> = ftl_paths.iter().map(|p| *p).collect();

            let report = validate::validate(json_path, &ftl_refs)?;
            validate::print_report(&report);
        }

        Commands::Mangle { ftl, write } => {
            let mut reports = Vec::new();

            for ftl_path in &ftl {
                let path = Path::new(ftl_path);
                let report = mangle::analyze_ftl(path)?;

                if write && !report.ids_to_mangle.is_empty() {
                    let mangled_content = mangle::mangle_ftl(path)?;
                    fs::write(path, mangled_content)?;
                }

                reports.push(report);
            }

            mangle::print_report(&reports, !write);
        }
    }

    Ok(())
}
