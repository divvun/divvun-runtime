use std::path::Path;
use std::process::Command;

use tempfile::tempdir;

#[derive(Debug, thiserror::Error, miette::Diagnostic)]
pub enum Error {
    #[error("Deno execution failed: {0}")]
    DenoExecution(String),
    #[error("{0}")]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    Io(#[from] std::io::Error),
}

pub fn dump_ast(input: &str) -> Result<serde_json::Value, Error> {
    let tmp = tempdir()?;

    // Write the pipeline code to a file so it can be imported
    std::fs::write(tmp.path().join("pipeline.ts"), input)?;

    // Generate TypeScript runtime modules
    match divvun_runtime::ts::generate(tmp.path().join(".divvun-rt")) {
        Ok(_) => {}
        Err(e) => {
            eprintln!("Failed to generate TypeScript modules: {:?}", e);
            return Err(Error::Io(e));
        }
    }

    // Create a wrapper TypeScript file that imports the pipeline and exports the AST
    let wrapper_content = r#"
import { toKebabCase } from "jsr:@std/text/to-kebab-case";
import { StringEntry, Ref, _current } from './.divvun-rt/mod.ts';
import * as pipelineModule from './pipeline.ts';

const pipelines: { [key: string]: any } = {};
let defaultPipelineName: string | null = null;

// Process all exports (both default and named)
for (const [exportName, fn] of Object.entries(pipelineModule)) {
    if (typeof fn !== 'function') continue;
    if (!fn.name) continue;  // Skip anonymous functions

    // Check if function name ends with _dev
    const isDev = fn.name.endsWith('_dev');

    // Strip _dev suffix before converting to kebab-case
    const cleanName = isDev ? fn.name.slice(0, -4) : fn.name;
    const name = toKebabCase(cleanName);

    _current.clear();
    const entry = new StringEntry();
    const output = fn(entry);
    const commands: { [key: string]: any } = {};

    for (const [id, command] of _current.entries()) {
        if (Array.isArray(command.input)) {
            command.input = command.input.map(x => new Ref(x));
        } else if (command.input) {
            command.input = new Ref(command.input);
        }
        commands[id] = command;
    }

    pipelines[name] = {
        entry,
        output: new Ref(output),
        commands,
        dev: isDev
    };

    // Mark which one is the default export
    if (exportName === 'default') {
        defaultPipelineName = name;
    }
}

if (Object.keys(pipelines).length === 0) {
    throw new Error("No pipeline functions found!");
}

// If no default export, use the first pipeline
if (!defaultPipelineName) {
    defaultPipelineName = Object.keys(pipelines)[0];
}

const result = {
    version: 1,
    default: defaultPipelineName,
    pipelines
};

console.log(JSON.stringify(result));
"#;

    let wrapper_path = tmp.path().join("wrapper.ts");
    std::fs::write(&wrapper_path, wrapper_content)?;

    // Execute with Deno
    let output = Command::new("deno")
        .args(&["run", "--allow-read"])
        .arg(&wrapper_path)
        .current_dir(tmp.path())
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Error::DenoExecution(format!(
            "Exit code: {}, stderr: {}",
            output.status.code().unwrap_or(-1),
            stderr
        )));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json_value: serde_json::Value = serde_json::from_str(&stdout)?;

    Ok(json_value)
}

pub fn save_ast(path: impl AsRef<Path>, output: impl AsRef<Path>) -> Result<(), Error> {
    let mut path = path.as_ref().to_path_buf();
    if !path.ends_with(".ts") {
        path = path.join("pipeline.ts");
    }
    let input = std::fs::read_to_string(path)?;
    let res = dump_ast(&input)?;
    std::fs::write(output, serde_json::to_string(&res)?)?;
    Ok(())
}
