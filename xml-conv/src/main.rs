use anyhow::Result;
use clap::{ArgAction, Parser, ValueEnum};
use std::fs;
use std::io::{self, Write};
use std::path::Path;
use xml_conv::{fluent, kdl, parse_xml_to_errors};

#[derive(ValueEnum, Clone, Debug)]
enum OutputFormat {
    Json,
    Fluent,
    Kdl,
}

#[derive(Parser)]
#[command(name = "xml-conv")]
#[command(about = "Convert XML files to JSON or Fluent format")]
#[command(version)]
struct Cli {
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
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let xml_content = fs::read_to_string(&cli.input)?;
    let error_doc = parse_xml_to_errors(&xml_content)?;

    match cli.format {
        OutputFormat::Json => {
            let json_output = if cli.pretty {
                serde_json::to_string_pretty(&error_doc)?
            } else {
                serde_json::to_string(&error_doc)?
            };

            match cli.output {
                Some(output_path) => {
                    fs::write(&output_path, json_output)?;
                    println!("Converted {} to {}", cli.input, output_path);

                    // Generate errors.json metadata file alongside JSON output
                    let output_dir = Path::new(&output_path).parent().unwrap_or(Path::new("."));
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
            let output_dir = cli.output.as_deref().unwrap_or(".");
            let output_path = Path::new(output_dir);

            fluent::write_fluent_files(&error_doc, output_path)?;
            println!("Converted {} to Fluent files in {}", cli.input, output_dir);
        }
        OutputFormat::Kdl => {
            let kdl_output = kdl::to_kdl(&error_doc)?;

            match cli.output {
                Some(output_path) => {
                    fs::write(&output_path, &kdl_output)?;
                    println!("Converted {} to {}", cli.input, output_path);
                }
                None => {
                    print!("{}", kdl_output);
                    io::stdout().flush()?;
                }
            }
        }
    }

    Ok(())
}
