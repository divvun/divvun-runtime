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

/// The wrapper Deno runs: imports the runtime bindings and the pipeline by
/// absolute file URL (tokens replaced at runtime), collects the command
/// registry, and prints the AST as JSON.
const WRAPPER_TEMPLATE: &str = r#"
import { toKebabCase } from "jsr:@std/text/to-kebab-case";
import { StringEntry, Ref, _current } from '__RT_MOD_URL__';
import * as pipelineModule from '__PIPELINE_URL__';

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

/// A file URL for an absolute path, with just enough percent-encoding for a
/// module specifier to survive spaces and URL metacharacters in the path.
fn file_url(path: &Path) -> String {
    let text = path.to_string_lossy().replace('\\', "/");
    let encoded = text
        .replace('%', "%25")
        .replace(' ', "%20")
        .replace('#', "%23")
        .replace('?', "%3F");
    if encoded.starts_with('/') {
        format!("file://{}", encoded)
    } else {
        format!("file:///{}", encoded)
    }
}

pub fn dump_ast(pipeline_path: impl AsRef<Path>) -> Result<serde_json::Value, Error> {
    // The pipeline runs IN PLACE, not from a tempdir copy, so its own
    // relative imports resolve - a JSON config beside it, a sibling helper
    // module. Copying the source text into a tempdir made every such import
    // dangle at bundle time while `deno check` (run against the real file)
    // passed, which is the worst place for the failure to appear.
    let mut pipeline_path = std::fs::canonicalize(pipeline_path.as_ref())?;
    if pipeline_path.is_dir() {
        pipeline_path = pipeline_path.join("pipeline.ts");
    }
    let pipeline_dir = pipeline_path
        .parent()
        .ok_or_else(|| {
            Error::Io(std::io::Error::other(
                "pipeline path has no parent directory",
            ))
        })?
        .to_path_buf();

    // Regenerate the runtime bindings beside the pipeline, where its own
    // `./.divvun-rt/` imports resolve. The wrapper below must import the SAME
    // mod.ts module instance the pipeline imports - a second copy would have
    // its own empty `_current` registry - so both import from here.
    let rt_dir = pipeline_dir.join(".divvun-rt");
    match std::fs::remove_dir_all(&rt_dir) {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(Error::Io(e)),
    }
    match divvun_runtime::ts::generate(&rt_dir) {
        Ok(_) => {}
        Err(e) => {
            eprintln!("Failed to generate TypeScript modules: {:?}", e);
            return Err(Error::Io(e));
        }
    }

    // The wrapper still lives in a tempdir so nothing is written next to the
    // user's pipeline beyond the bindings; it reaches both real files by URL.
    let tmp = tempdir()?;
    let wrapper_content = WRAPPER_TEMPLATE
        .replace("__RT_MOD_URL__", &file_url(&rt_dir.join("mod.ts")))
        .replace("__PIPELINE_URL__", &file_url(&pipeline_path));

    let wrapper_path = tmp.path().join("wrapper.ts");
    std::fs::write(&wrapper_path, wrapper_content)?;

    // Execute with Deno
    let output = Command::new("deno")
        .args(&["run", "--allow-read"])
        .arg(&wrapper_path)
        .current_dir(&pipeline_dir)
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
    if path.is_dir() {
        path = path.join("pipeline.ts");
    }
    let res = dump_ast(&path)?;
    std::fs::write(output, serde_json::to_string(&res)?)?;
    Ok(())
}
