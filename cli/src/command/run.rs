use std::{io::{IsTerminal, Read, Write as _}, sync::Arc};

#[cfg(unix)]
use std::os::unix::ffi::OsStrExt;

use divvun_runtime::{ast::Command, modules::Input, Bundle};
use pathos::AppDirs;
use rustyline::error::ReadlineError;

use crate::{
    cli::{DebugDumpAstArgs, RunArgs},
    shell::Shell,
};

pub fn dump_ast(shell: &mut Shell, args: DebugDumpAstArgs) -> anyhow::Result<()> {
    let value = divvun_runtime::py_rt::dump_ast(&std::fs::read_to_string(args.path)?)?;
    println!("{}", serde_json::to_string_pretty(&value).unwrap());
    Ok(())
}

fn tap((i, j): (usize, usize), cmd: &Command, input: &Input) {
    match input {
        Input::String(s) => println!("[{i}] {cmd}\n{s}"),
        Input::Bytes(b) => println!("[{i}] {cmd}\nbytes: {}", b.len()),
        Input::Json(j) => println!("[{i}] {cmd}\n{}", serde_json::to_string_pretty(j).unwrap()),
        Input::Multiple(x) => {
            for (n, input) in x.iter().enumerate() {
                print!("[{n}]:");
                tap((i, j), &cmd, input);
            }
        }
    }
}

fn step_tap((i, j): (usize, usize), cmd: &Command, input: &Input) {
    tap((i, j), cmd, input);
    if i + 1 < j {
        print!("[{i}] <->");
        std::io::stdout().flush().unwrap();
        std::io::stdin().lines().next();
    }
}

async fn run_repl(
    shell: &mut Shell,
    bundle: &Bundle,
    args: &RunArgs,
) -> Result<(), Arc<anyhow::Error>> {
    let dirs = pathos::user::AppDirs::new("Divvun Runtime").map_err(|x| Arc::new(x.into()))?;
    std::fs::create_dir_all(dirs.data_dir()).map_err(|x| Arc::new(x.into()))?;

    let history_path = dirs.data_dir().join("repl_history");

    let mut rl = rustyline::DefaultEditor::new().map_err(|e| Arc::new(e.into()))?;
    if rl.load_history(&history_path).is_err() {
        // Do nothing
    }

    let mut is_stepping = false;

    println!(
        "Divvun Runtime v{} - type /help for commands",
        env!("CARGO_PKG_VERSION")
    );

    loop {
        let readline = rl.readline(">> ");
        match readline {
            Ok(line) => {
                let line = line.trim();
                if line.starts_with("/") {
                    match &*line {
                        "/list" => {
                            for (i, v) in bundle.definition().commands.values().enumerate() {
                                println!("{i}: {v}");
                            }
                            println!();
                        }
                        "/ast" => {
                            println!(
                                "{}\n",
                                serde_json::to_string_pretty(&**bundle.definition()).unwrap()
                            );
                        }
                        "/step" => {
                            is_stepping = !is_stepping;
                            if is_stepping {
                                shell.status("Stepping", "enabled")?;
                            } else {
                                shell.status("Stepping", "disabled")?;
                            }
                        }
                        unknown => {
                            shell.error(format!("Unknown command: {}", unknown))?;
                        }
                    }
                    continue;
                }

                let result = if is_stepping {
                    bundle
                        .run_pipeline_with_tap(Input::String(line.to_string()), step_tap)
                        .await
                        .map_err(|e| Arc::new(e.into()))?
                } else {
                    bundle
                        .run_pipeline_with_tap(Input::String(line.to_string()), tap)
                        .await
                        .map_err(|e| Arc::new(e.into()))?
                };

                rl.add_history_entry(line).map_err(|e| Arc::new(e.into()))?;

                if let Some(path) = args.output_path.as_deref() {
                    match result {
                        Input::Multiple(_) => todo!("multiple not supported"),
                        Input::String(s) => {
                            std::fs::write(path, s).map_err(|e| Arc::new(e.into()))?
                        }
                        Input::Bytes(b) => {
                            std::fs::write(path, b).map_err(|e| Arc::new(e.into()))?
                        }
                        Input::Json(j) => std::fs::write(
                            path,
                            serde_json::to_string_pretty(&j).map_err(|e| Arc::new(e.into()))?,
                        )
                        .map_err(|e| Arc::new(e.into()))?,
                    }

                    if let Some(app) = args.command.as_deref() {
                        if cfg!(windows) {
                            std::process::Command::new("pwsh")
                                .arg("-c")
                                .arg(format!("{app} {}", path.display()))
                                .spawn()
                                .unwrap()
                                .wait()
                                .map_err(|e| Arc::new(e.into()))?;
                        } else {
                            std::process::Command::new("sh")
                                .arg("-c")
                                .arg(format!("{app} {}", path.display()))
                                .spawn()
                                .unwrap()
                                .wait()
                                .map_err(|e| Arc::new(e.into()))?;
                        }
                    }
                } else {
                    println!()
                }
            }
            Err(ReadlineError::Interrupted) => {
                break;
            }
            Err(ReadlineError::Eof) => {
                break;
            }
            Err(err) => {
                shell.error(err)?;
                break;
            }
        }
    }

    rl.save_history(&history_path)
        .map_err(|e| Arc::new(e.into()))?;

    Ok(())
}

pub async fn run(shell: &mut Shell, mut args: RunArgs) -> Result<(), Arc<anyhow::Error>> {
    let bundle = if args.path.extension().map(|x| x.as_encoded_bytes()) == Some(b"drb") {
        Bundle::from_bundle(&args.path).map_err(|e| Arc::new(e.into()))?
    } else {
        Bundle::from_path(&args.path).map_err(|e| Arc::new(e.into()))?
    };

    if !std::io::stdin().is_terminal() {
        println!("AHAHAHAHAHA");
        let mut s = String::new();
        std::io::stdin().read_to_string(&mut s).map_err(|e| Arc::new(e.into()))?;
        args.input = Some(s);
    } else {
        println!("NOT A TERMINAL");
    }

    if let Some(input) = args.input {
        let result = bundle
            .run_pipeline_with_tap(Input::String(input), tap)
            .await
            .map_err(|e| Arc::new(e.into()))?;

        if let Some(path) = args.output_path.as_deref() {
            match result {
                Input::Multiple(_) => todo!("multiple not supported"),
                Input::String(s) => std::fs::write(path, s).map_err(|e| Arc::new(e.into()))?,
                Input::Bytes(b) => std::fs::write(path, b).map_err(|e| Arc::new(e.into()))?,
                Input::Json(j) => std::fs::write(
                    path,
                    serde_json::to_string_pretty(&j).map_err(|e| Arc::new(e.into()))?,
                )
                .map_err(|e| Arc::new(e.into()))?,
            }
            println!("Wrote to {}", path.display());
            if let Some(app) = args.command.as_deref() {
                if cfg!(windows) {
                    std::process::Command::new("pwsh")
                        .arg("-c")
                        .arg(format!("{app} {}", path.display()))
                        .spawn()
                        .unwrap()
                        .wait()
                        .map_err(|e| Arc::new(e.into()))?;
                } else {
                    std::process::Command::new("sh")
                        .arg("-c")
                        .arg(format!("{app} {}", path.display()))
                        .spawn()
                        .unwrap()
                        .wait()
                        .map_err(|e| Arc::new(e.into()))?;
                }
            }
        }
    } else {
        run_repl(shell, &bundle, &args).await?;
    }

    Ok(())
}
