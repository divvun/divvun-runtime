use std::{
    collections::HashMap,
    io::{IsTerminal, Read, Write as _},
    sync::Arc,
};

#[cfg(unix)]
use std::os::unix::ffi::OsStrExt;

use divvun_runtime::{ast::Command, modules::Input, Bundle};
use futures_util::StreamExt;
use pathos::AppDirs;
use rustyline::error::ReadlineError;
use serde_json::Map;
use termcolor::Color;

use crate::{
    cli::{DebugDumpAstArgs, RunArgs},
    shell::Shell,
};

pub fn dump_ast(shell: &mut Shell, args: DebugDumpAstArgs) -> anyhow::Result<()> {
    let value = crate::py_rt::dump_ast(&std::fs::read_to_string(args.path)?)?;
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
        Input::ArrayString(x) => {
            for (n, input) in x.iter().enumerate() {
                print!("[{n}]:");
                tap((i, j), &cmd, &Input::String(input.clone()));
            }
        }
        Input::ArrayBytes(x) => {
            for (n, input) in x.iter().enumerate() {
                print!("[{n}]:");
                tap((i, j), &cmd, &Input::Bytes(input.clone()));
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
        "Divvun Runtime v{} - type :help for commands",
        env!("CARGO_PKG_VERSION")
    );

    let mut config = parse_config(&args.config)?;
    let mut pipe = bundle
        .create(config.clone())
        .await
        .map_err(|e| Arc::new(e.into()))?;

    loop {
        let readline = rl.readline(">> ");
        match readline {
            Ok(line) => {
                let line = line.trim();
                if line.starts_with(":") {
                    let mut chunks = line.split_ascii_whitespace();
                    let command = chunks.next().unwrap();

                    match command {
                        ":help" => {
                            println!("Available commands:");
                            println!(":help - Display this help message");
                            println!(":list - List all available modules");
                            println!(":step - Enable/disable stepping through pipeline");
                            println!(":ast - Display the parsed AST");
                            println!(":config - Display the current configuration");
                            println!(":set [var] [value] - Set a configuration variable");
                            println!(":exit - Exit the REPL");
                            println!();
                        }
                        ":exit" => {
                            std::process::exit(0);
                        }
                        ":list" => {
                            for (i, v) in bundle.definition().commands.values().enumerate() {
                                println!("{i}: {v}");
                            }
                            println!();
                        }
                        ":ast" => {
                            println!(
                                "{}\n",
                                serde_json::to_string_pretty(&**bundle.definition()).unwrap()
                            );
                        }
                        ":step" => {
                            is_stepping = !is_stepping;
                            if is_stepping {
                                shell.status("Stepping", "enabled")?;
                            } else {
                                shell.status("Stepping", "disabled")?;
                            }
                        }
                        ":config" => {
                            println!("{}\n", serde_json::to_string_pretty(&config).unwrap());
                        }
                        ":set" => {
                            let Some(var) = chunks.next() else {
                                shell.error("Missing variable name")?;
                                continue;
                            };
                            let Some(value) = chunks.next() else {
                                shell.error("Missing variable value")?;
                                continue;
                            };
                            let value = match serde_json::from_str::<serde_json::Value>(value) {
                                Ok(v) => v,
                                Err(e) => {
                                    shell.error(format!("Failed to parse value: {}", e))?;
                                    continue;
                                }
                            };

                            shell.status("Setting", format!("{var} = {value:?}"))?;
                            config
                                .as_object_mut()
                                .unwrap()
                                .insert(var.to_string(), value);
                            pipe = bundle
                                .create(config.clone())
                                .await
                                .map_err(|e| Arc::new(e.into()))?;
                        }
                        unknown => {
                            shell.error(format!("Unknown command: {}", unknown))?;
                        }
                    }
                    continue;
                }

                rl.add_history_entry(line).map_err(|e| Arc::new(e.into()))?;

                // let result = if is_stepping {
                //     bundle
                //         .run_pipeline_with_tap(
                //             Input::String(line.to_string()),
                //             config.clone(),
                //             step_tap,
                //         )
                //         .await
                //         .map_err(|e| Arc::new(e.into()))?
                // } else {
                //     bundle
                //         .run_pipeline_with_tap(Input::String(line.to_string()), config.clone(), tap)
                //         .await
                //         .map_err(|e| Arc::new(e.into()))?
                // };
                let mut stream = pipe.forward(Input::String(line.to_string())).await;

                while let Some(input) = stream.next().await {
                    match input {
                        Ok(input) => {
                            shell.print(
                                &"<-",
                                Some(&format!("{:#}", input)),
                                Color::Green,
                                false,
                            )?;

                            if let Some(path) = args.output_path.as_deref() {
                                match input {
                                    Input::Multiple(_) => todo!("multiple not supported"),
                                    Input::String(s) => {
                                        std::fs::write(path, s).map_err(|e| Arc::new(e.into()))?
                                    }
                                    Input::Bytes(b) => {
                                        std::fs::write(path, b).map_err(|e| Arc::new(e.into()))?
                                    }
                                    Input::Json(j) => std::fs::write(
                                        path,
                                        serde_json::to_string_pretty(&j)
                                            .map_err(|e| Arc::new(e.into()))?,
                                    )
                                    .map_err(|e| Arc::new(e.into()))?,
                                    Input::ArrayString(x) => todo!("multiple not supported"),
                                    Input::ArrayBytes(x) => todo!("multiple not supported"),
                                }

                                if let Some(app) = args.command.as_deref() {
                                    shell.status_with_color(
                                        "Running",
                                        format!("{app} {}", path.display()),
                                        Color::Cyan,
                                    )?;
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
                                    shell.err_erase_line();
                                }
                            }
                        }
                        Err(e) => {
                            tracing::error!("{}", e);
                        }
                    }
                }

                tracing::debug!("DONE");
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

fn parse_config(config: &[String]) -> Result<serde_json::Value, anyhow::Error> {
    tracing::debug!("Parsing config: {:?}", config);
    let map = config
        .iter()
        .map(|x| {
            let mut arr = x.splitn(2, '=');
            let Some(a) = arr.next() else {
                anyhow::bail!("Invalid input: {}", x);
            };
            let Some(b) = arr.next() else {
                anyhow::bail!("Invalid input: {}", x);
            };
            serde_json::from_str::<'_, serde_json::Value>(b)
                .map_err(|e| e.into())
                .map(|b| (a.to_string(), b))
        })
        .collect::<Result<Map<_, _>, _>>()?;

    Ok(serde_json::Value::Object(map))
}

pub async fn run(shell: &mut Shell, mut args: RunArgs) -> Result<(), Arc<anyhow::Error>> {
    let path = args
        .path
        .as_ref()
        .cloned()
        .unwrap_or_else(|| std::env::current_dir().unwrap());
    let bundle = if path.extension().map(|x| x.as_encoded_bytes()) == Some(b"drb") {
        Bundle::from_bundle(&path).map_err(|e| Arc::new(e.into()))?
    } else {
        crate::py_rt::save_ast(&path, "pipeline.json").map_err(|e| Arc::new(e.into()))?;
        Bundle::from_path(&path).map_err(|e| Arc::new(e.into()))?
    };

    let config = parse_config(&args.config)?;

    if !std::io::stdin().is_terminal() {
        // println!("AHAHAHAHAHA");
        let mut s = String::new();
        std::io::stdin()
            .read_to_string(&mut s)
            .map_err(|e| Arc::new(e.into()))?;
        args.input = Some(s);
    } else {
        // println!("NOT A TERMINAL");
    }

    let mut pipe = bundle
        .create(config)
        .await
        .map_err(|e| Arc::new(e.into()))?;

    if let Some(input) = args.input {
        let mut stream = pipe.forward(Input::String(input)).await;

        while let Some(Ok(input)) = stream.next().await {
            println!("{:?}", input);
        }

        // if let Some(path) = args.output_path.as_deref() {
        //     match result {
        //         Input::Multiple(_) => todo!("multiple not supported"),
        //         Input::String(s) => std::fs::write(path, s).map_err(|e| Arc::new(e.into()))?,
        //         Input::Bytes(b) => std::fs::write(path, b).map_err(|e| Arc::new(e.into()))?,
        //         Input::Json(j) => std::fs::write(
        //             path,
        //             serde_json::to_string_pretty(&j).map_err(|e| Arc::new(e.into()))?,
        //         )
        //         .map_err(|e| Arc::new(e.into()))?,
        //         Input::ArrayString(x) => todo!("multiple not supported"),
        //         Input::ArrayBytes(x) => todo!("multiple not supported"),
        //     }
        //     println!("Wrote to {}", path.display());
        //     if let Some(app) = args.command.as_deref() {
        //         if cfg!(windows) {
        //             std::process::Command::new("pwsh")
        //                 .arg("-c")
        //                 .arg(format!("{app} {}", path.display()))
        //                 .spawn()
        //                 .unwrap()
        //                 .wait()
        //                 .map_err(|e| Arc::new(e.into()))?;
        //         } else {
        //             std::process::Command::new("sh")
        //                 .arg("-c")
        //                 .arg(format!("{app} {}", path.display()))
        //                 .spawn()
        //                 .unwrap()
        //                 .wait()
        //                 .map_err(|e| Arc::new(e.into()))?;
        //         }
        //     }
        // }
    } else {
        run_repl(shell, &bundle, &args).await?;
    }

    Ok(())
}
