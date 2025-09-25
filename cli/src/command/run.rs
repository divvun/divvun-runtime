use std::{
    io::{IsTerminal, Read},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};

use divvun_runtime::{
    ast::Command,
    modules::{Input, InputEvent, TapOutput},
    Bundle,
};
use futures_util::{FutureExt, StreamExt};
use pathos::AppDirs;
use rustyline::error::ReadlineError;
use serde_json::Map;
use termcolor::Color;
use tokio::{io::AsyncReadExt as _, sync::RwLock};

use crate::{
    cli::{DebugDumpAstArgs, RunArgs},
    shell::Shell,
};

use super::utils;

pub fn dump_ast(_shell: &mut Shell, args: DebugDumpAstArgs) -> anyhow::Result<()> {
    let value = crate::deno_rt::dump_ast(&std::fs::read_to_string(args.path)?)?;
    println!("{}", serde_json::to_string_pretty(&value).unwrap());
    Ok(())
}

// fn tap((i, j): (usize, usize), cmd: &Command, input: &Input) {
//     match input {
//         Input::String(s) => println!("[{i}] {cmd}\n{s}"),
//         Input::Bytes(b) => println!("[{i}] {cmd}\nbytes: {}", b.len()),
//         Input::Json(j) => println!("[{i}] {cmd}\n{}", serde_json::to_string_pretty(j).unwrap()),
//         Input::Multiple(x) => {
//             for (n, input) in x.iter().enumerate() {
//                 print!("[{n}]:");
//                 tap((i, j), &cmd, input);
//             }
//         }
//         Input::ArrayString(x) => {
//             for (n, input) in x.iter().enumerate() {
//                 print!("[{n}]:");
//                 tap((i, j), &cmd, &Input::String(input.clone()));
//             }
//         }
//         Input::ArrayBytes(x) => {
//             for (n, input) in x.iter().enumerate() {
//                 print!("[{n}]:");
//                 tap((i, j), &cmd, &Input::Bytes(input.clone()));
//             }
//         }
//     }
// }

// fn step_tap((i, j): (usize, usize), cmd: &Command, input: &Input) {
//     tap((i, j), cmd, input);
//     if i + 1 < j {
//         print!("[{i}] <->");
//         std::io::stdout().flush().unwrap();
//         std::io::stdin().lines().next();
//     }
// }

#[derive(Clone)]
struct TapEvent {
    key: String,
    command: Command,
    event: InputEvent,
}

#[derive(Clone)]
struct PipelineRun {
    input: String,
    events: Vec<TapEvent>,
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

    let is_stepping = Arc::new(AtomicBool::new(false));

    println!(
        "Divvun Runtime v{} - type :help for commands",
        env!("CARGO_PKG_VERSION")
    );

    let mut config = parse_config(&args.config)?;
    let breakpoint: Arc<RwLock<Option<String>>> = Arc::new(RwLock::new(None));

    // Buffer to store the last pipeline run
    let last_run: Arc<Mutex<Option<PipelineRun>>> = Arc::new(Mutex::new(None));
    let current_events: Arc<Mutex<Vec<TapEvent>>> = Arc::new(Mutex::new(Vec::new()));
    let current_events_clone = current_events.clone();

    let tap_stepping = is_stepping.clone();
    let tap_breakpoint = breakpoint.clone();
    let tap = Arc::new(move |key: &str, cmd: &Command, event: &InputEvent| {
        let current_events_clone = current_events_clone.clone();
        let tap_breakpoint = tap_breakpoint.clone();
        let tap_stepping = tap_stepping.clone();

        println!("\x1b[41;31m[{}]\x1b[0m {}", key, cmd);
        println!("\x1b[33m{:#}\x1b[0m", event);

        // Store the event for the current run
        if let Ok(mut events) = current_events_clone.lock() {
            events.push(TapEvent {
                key: key.to_string(),
                command: cmd.clone(),
                event: event.clone(),
            });
        }

        let key = key.to_string();

        async move {
            if tap_breakpoint.read().await.as_deref() == Some(&key) {
                println!("\x1b[45;35m[{}]\x1b[0m <-> [Breakpoint hit]", key);
                TapOutput::Stop
            } else if tap_stepping.load(Ordering::Relaxed) {
                use crossterm::terminal;
                println!(
                    "\x1b[44;34m[{}]\x1b[0m <-> [Any to continue, Esc to stop]",
                    key
                );

                terminal::enable_raw_mode().unwrap();
                let buf = tokio::io::stdin().read_u8().await.unwrap();
                terminal::disable_raw_mode().unwrap();

                if buf == 0x1B {
                    TapOutput::Stop
                } else {
                    TapOutput::Continue
                }
            } else {
                TapOutput::Continue
            }
        }
        .boxed()
    });

    let mut pipe = bundle
        .create_with_tap(config.clone(), tap.clone())
        .await
        .map_err(|e| Arc::new(e.into()))?;

    loop {
        let readline = rl.readline(">> ");
        let line = match readline {
            Ok(line) => {
                rl.add_history_entry(&line)
                    .map_err(|e| Arc::new(e.into()))?;

                line
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
        };

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
                    println!(":set [id] [value] - Set a configuration variable");
                    println!(":breakpoint [command_id|clear] - Set/clear breakpoint at command");
                    println!(":save [filename] - Export last run as markdown");
                    println!(":exit - Exit the REPL");
                    println!();
                }
                ":exit" => {
                    std::process::exit(0);
                }
                ":list" => {
                    for (id, command) in bundle.definition().commands.iter() {
                        println!("{}: {}", id, command);
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
                    let cur = is_stepping
                        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |v| Some(!v))
                        .unwrap();

                    if !cur {
                        shell.status("Stepping", "enabled")?;
                    } else {
                        shell.status("Stepping", "disabled")?;
                    }
                }
                ":config" => {
                    println!("{}\n", serde_json::to_string_pretty(&config).unwrap());
                }
                ":save" => {
                    let filename = chunks.next().unwrap_or("pipeline_debug.md");
                    match save_markdown(&last_run, filename) {
                        Ok(()) => shell.status("Saved", format!("Debug log to {}", filename))?,
                        Err(e) => shell.error(format!("Failed to save: {}", e))?,
                    }
                }
                ":set" => {
                    let Some(var) = chunks.next() else {
                        shell.error("Missing id name")?;
                        continue;
                    };
                    let value = chunks.collect::<Vec<_>>().join(" ");
                    let value = match serde_json::from_str::<serde_json::Value>(&value) {
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
                        .create_with_tap(config.clone(), tap.clone())
                        .await
                        .map_err(|e| Arc::new(e.into()))?;
                }
                ":breakpoint" => {
                    let arg = chunks.next();
                    let mut breakpoint_guard = breakpoint.write().await;
                    match arg {
                        Some("clear") | None => {
                            *breakpoint_guard = None;
                            shell.status("Breakpoint", "cleared")?;
                        }
                        Some(id) => {
                            if bundle.definition().commands.contains_key(id) {
                                *breakpoint_guard = Some(id.to_string());
                                shell.status("Breakpoint", format!("set at command '{}'", id))?;
                            } else {
                                shell.error(format!("Command '{}' not found", id))?;
                                continue;
                            }
                        }
                    }
                }
                unknown => {
                    shell.error(format!("Unknown command: {}", unknown))?;
                }
            }
            continue;
        }

        // Clear the events for the new run
        if let Ok(mut events) = current_events.lock() {
            events.clear();
        }

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
                    shell.print(&"<-", Some(&format!("{:#}", input)), Color::Green, false)?;

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
                                serde_json::to_string_pretty(&j).map_err(|e| Arc::new(e.into()))?,
                            )
                            .map_err(|e| Arc::new(e.into()))?,
                            Input::ArrayString(_) => todo!("multiple not supported"),
                            Input::ArrayBytes(_) => todo!("multiple not supported"),
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

        // Save the completed run for potential export
        if let Ok(events) = current_events.lock() {
            if !events.is_empty() {
                if let Ok(mut run) = last_run.lock() {
                    *run = Some(PipelineRun {
                        input: line.to_string(),
                        events: events.clone(),
                    });
                }
            }
        }

        tracing::debug!("DONE");
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

fn strip_ansi_codes(s: &str) -> String {
    // Simple ANSI escape sequence removal
    use regex::Regex;
    let re = Regex::new(r"\x1b\[[0-9;]*m").unwrap();
    re.replace_all(s, "").to_string()
}

fn save_markdown(
    last_run: &Arc<Mutex<Option<PipelineRun>>>,
    filename: &str,
) -> Result<(), anyhow::Error> {
    use std::fmt::Write;

    let run = match last_run.lock() {
        Ok(run) => run.clone(),
        Err(_) => return Err(anyhow::anyhow!("Failed to acquire lock on pipeline run")),
    };

    let run = match run {
        Some(run) => run,
        None => return Err(anyhow::anyhow!("No pipeline run to export")),
    };

    let mut markdown = String::new();

    writeln!(markdown, "# Pipeline Debug Report")?;
    writeln!(markdown)?;
    writeln!(markdown, "## Input")?;
    writeln!(markdown, "```")?;
    writeln!(markdown, "{}", run.input)?;
    writeln!(markdown, "```")?;
    writeln!(markdown)?;
    writeln!(markdown, "## Pipeline Execution")?;
    writeln!(markdown)?;

    for event in &run.events {
        let command_str = strip_ansi_codes(&format!("{}", event.command));
        let event_str = strip_ansi_codes(&format!("{:#}", event.event));

        writeln!(markdown, "<details>")?;
        writeln!(
            markdown,
            "<summary><code>[{}]</code> <code>{}</code></summary>",
            event.key, command_str
        )?;
        writeln!(markdown)?;
        writeln!(markdown, "```")?;
        writeln!(markdown, "{}", event_str)?;
        writeln!(markdown, "```")?;
        writeln!(markdown, "</details>")?;
        writeln!(markdown)?;
    }

    std::fs::write(filename, markdown)?;

    Ok(())
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
        // For TypeScript files, prepare the environment (sync + type check)
        let pipeline_path = if path.ends_with(".ts") {
            path.clone()
        } else {
            path.join("pipeline.ts")
        };

        if pipeline_path.exists() {
            utils::prepare_typescript_pipeline(shell, &pipeline_path, args.skip_check)
                .map_err(|e| Arc::new(e))?;
        }

        crate::deno_rt::save_ast(&path, "pipeline.json").map_err(|e| Arc::new(e.into()))?;
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
