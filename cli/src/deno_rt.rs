use std::path::Path;
use std::process::Command;

use tempfile::tempdir;

use divvun_runtime::ast::PipelineDefinition;

#[derive(Debug, thiserror::Error)]
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
    let wrapper_content = format!(
        r#"
import {{ StringEntry, Ref, _current }} from './.divvun-rt/mod.ts';
import pipeline from './pipeline.ts';

if (typeof pipeline === 'function') {{
    const entry = new StringEntry();
    const output = pipeline(entry);
    const commands: {{ [key: string]: any }} = {{}};
    
    for (const [id, command] of _current.entries()) {{
        // Convert inputs to refs
        if (Array.isArray(command.input)) {{
            command.input = command.input.map(x => new Ref(x));
        }} else if (command.input) {{
            command.input = new Ref(command.input);
        }}
        
        commands[id] = command;
    }}
    
    const result = {{
        entry,
        output: new Ref(output),
        commands
    }};
    
    console.log(JSON.stringify(result));
}} else {{
    throw new Error("No pipeline found!");
}}
"#
    );

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

pub fn interpret_pipeline(input: &str) -> Result<PipelineDefinition, Error> {
    let res = dump_ast(input)?;
    let pd: PipelineDefinition = serde_json::from_value(res)?;
    Ok(pd)
}

pub fn save_ast(path: impl AsRef<Path>, output: &str) -> Result<(), Error> {
    let mut path = path.as_ref().to_path_buf();
    if !path.ends_with(".ts") {
        path = path.join("pipeline.ts");
    }
    let input = std::fs::read_to_string(path)?;
    let res = dump_ast(&input)?;
    std::fs::write(output, serde_json::to_string(&res)?)?;
    Ok(())
}
