use std::{
    borrow::Cow,
    io::{self, IsTerminal, Read, Write},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

use miette::IntoDiagnostic;

use divvun_runtime::{
    ast::Command,
    bundle::Bundle,
    modules::{Input, InputEvent, TapOutput},
};
use futures_util::{FutureExt, StreamExt};
use pathos::AppDirs;
use rustyline::{
    Helper,
    completion::Completer,
    error::ReadlineError,
    highlight::{CmdKind, Highlighter},
    hint::Hinter,
    validate::Validator,
};
use serde_json::Map;
use termcolor::Color;
use tokio::{io::AsyncReadExt as _, sync::RwLock};

use crate::{
    cli::{DebugDumpAstArgs, RunArgs},
    shell::Shell,
};

use super::utils;

// Themed helper for rustyline that applies background/foreground colors
struct ThemedHelper {
    background: String,
    foreground: String,
}

impl ThemedHelper {
    fn new(colors: Option<&syntax_highlight::CommandColors>) -> Self {
        if let Some(colors) = colors {
            Self {
                background: colors.background.clone(),
                foreground: colors.foreground.clone(),
            }
        } else {
            Self {
                background: String::new(),
                foreground: String::new(),
            }
        }
    }
}

impl Completer for ThemedHelper {
    type Candidate = String;
}

impl Hinter for ThemedHelper {
    type Hint = String;
}

impl Validator for ThemedHelper {}

impl Highlighter for ThemedHelper {
    fn highlight_prompt<'b, 's: 'b, 'p: 'b>(
        &'s self,
        prompt: &'p str,
        _default: bool,
    ) -> Cow<'b, str> {
        if self.background.is_empty() {
            Cow::Borrowed(prompt)
        } else {
            Cow::Owned(format!("{}{}", self.background, prompt))
        }
    }

    fn highlight<'l>(&self, line: &'l str, _pos: usize) -> Cow<'l, str> {
        if self.background.is_empty() {
            Cow::Borrowed(line)
        } else {
            Cow::Owned(format!("{}{}{}", self.background, self.foreground, line))
        }
    }

    fn highlight_char(&self, _line: &str, _pos: usize, _kind: CmdKind) -> bool {
        !self.background.is_empty()
    }
}

impl Helper for ThemedHelper {}

fn format_input_highlighted(
    input: &Input,
    command: Option<&Command>,
    theme: Option<&str>,
    override_bg: Option<syntax_highlight::ThemeColor>,
) -> String {
    if !syntax_highlight::supports_color() {
        return format!("{:#}", input);
    }

    match input {
        Input::Json(j) => {
            let json = serde_json::to_string_pretty(j).unwrap();
            syntax_highlight::highlight_to_terminal_with_theme(&json, "json", theme, override_bg)
        }
        Input::String(s) => {
            let syntax = command
                .and_then(|cmd| cmd.kind.as_deref())
                .filter(|k| *k == "cg3");

            if let Some("cg3") = syntax {
                syntax_highlight::highlight_to_terminal_with_theme(s, "cg3", theme, override_bg)
            } else {
                s.clone()
            }
        }
        _ => format!("{:#}", input),
    }
}

fn print_input_highlighted(
    shell: &mut Shell,
    input: &Input,
    command: Option<&Command>,
) -> miette::Result<()> {
    let theme_bg = shell.theme().and_then(|theme_name| {
        syntax_highlight::get_theme_by_name(theme_name)
            .map(|theme| syntax_highlight::extract_command_colors(theme).1)
    });

    let formatted = format_input_highlighted(input, command, shell.theme(), theme_bg);
    io::Write::write_all(shell.out(), formatted.as_bytes()).into_diagnostic()?;
    writeln!(shell.out()).into_diagnostic()?;
    shell.out().flush().into_diagnostic()?;
    Ok(())
}

pub fn dump_ast(shell: &mut Shell, args: DebugDumpAstArgs) -> miette::Result<()> {
    let value = crate::deno_rt::dump_ast(&std::fs::read_to_string(args.path).into_diagnostic()?)?;
    let json = serde_json::to_string_pretty(&value).unwrap();
    shell
        .print_highlighted_stdout(&json, "json")
        .into_diagnostic()?;
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

async fn run_repl(shell: &mut Shell, bundle: &Bundle, args: &RunArgs) -> miette::Result<()> {
    let dirs = pathos::user::AppDirs::new("Divvun Runtime").into_diagnostic()?;
    std::fs::create_dir_all(dirs.data_dir()).into_diagnostic()?;

    let history_path = dirs.data_dir().join("repl_history");

    // Extract command colors from theme BEFORE creating editor
    let (cmd_colors, theme_bg) = shell
        .theme()
        .and_then(|theme_name| {
            syntax_highlight::get_theme_by_name(theme_name)
                .map(|theme| syntax_highlight::extract_command_colors(theme))
        })
        .map(|(colors, bg)| (Some(colors), Some(bg)))
        .unwrap_or((None, None));

    // Create themed editor
    let helper = ThemedHelper::new(cmd_colors.as_ref());
    let mut rl = rustyline::Editor::new().into_diagnostic()?;
    rl.set_helper(Some(helper));
    if rl.load_history(&history_path).is_err() {
        // Do nothing
    }

    let is_stepping = Arc::new(AtomicBool::new(false));

    // Print welcome message with theme background if available
    if let Some(ref colors) = cmd_colors {
        println!(
            "{}Divvun Runtime v{} - type :help for commands\x1b[K\x1b[0m",
            colors.background,
            env!("CARGO_PKG_VERSION")
        );
    } else {
        println!(
            "Divvun Runtime v{} - type :help for commands",
            env!("CARGO_PKG_VERSION")
        );
    }

    let mut config = parse_config(&args.config)?;
    let breakpoint: Arc<RwLock<Option<String>>> = Arc::new(RwLock::new(None));

    // Buffer to store the last pipeline run
    let last_run: Arc<Mutex<Option<PipelineRun>>> = Arc::new(Mutex::new(None));
    let current_events: Arc<Mutex<Vec<TapEvent>>> = Arc::new(Mutex::new(Vec::new()));
    let current_events_clone = current_events.clone();

    let tap_stepping = is_stepping.clone();
    let tap_breakpoint = breakpoint.clone();
    let theme = shell.theme().map(|s| s.to_string());

    // Prompt is simple - ThemedHelper::highlight_prompt applies theming
    let prompt = ">> ";

    // Clone cmd_colors before it's moved into the tap closure
    let output_cmd_colors = cmd_colors.clone();

    let tap = Arc::new(move |key: &str, cmd: &Command, event: &InputEvent| {
        let current_events_clone = current_events_clone.clone();
        let tap_breakpoint = tap_breakpoint.clone();
        let tap_stepping = tap_stepping.clone();
        let cmd_colors_clone = cmd_colors.clone();
        let theme_bg_clone = theme_bg;

        // Print with theme colors applied to background
        if let Some(ref colors) = cmd_colors_clone {
            println!(
                "{}[{}] {}\x1b[K\x1b[0m",
                colors.background,
                key,
                cmd.as_str(Some(colors))
            );
        } else {
            println!("[{}] {}", key, cmd.as_str(None));
        }

        match event {
            InputEvent::Input(input) => {
                let formatted =
                    format_input_highlighted(input, Some(cmd), theme.as_deref(), theme_bg_clone);
                // format_input_highlighted returns content with \x1b[K per line and final \x1b[0m
                println!("{}", formatted);
            }
            _ => {
                if let Some(ref colors) = cmd_colors_clone {
                    print!("{}{:#}\x1b[K", colors.background, event);
                    println!("\x1b[0m");
                } else {
                    println!("{:#}", event);
                }
            }
        }

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
                if let Some(ref colors) = cmd_colors_clone {
                    println!(
                        "{}[{}] <-> [Breakpoint hit]\x1b[K\x1b[0m",
                        colors.background, key
                    );
                } else {
                    println!("[{}] <-> [Breakpoint hit]", key);
                }
                TapOutput::Stop
            } else if tap_stepping.load(Ordering::Relaxed) {
                use crossterm::terminal;
                if let Some(ref colors) = cmd_colors_clone {
                    println!(
                        "{}[{}] <-> [Any to continue, Esc to stop]\x1b[K\x1b[0m",
                        colors.background, key
                    );
                } else {
                    println!("[{}] <-> [Any to continue, Esc to stop]", key);
                }

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
        .into_diagnostic()?;

    loop {
        let readline = rl.readline(&prompt);
        let line = match readline {
            Ok(line) => {
                rl.add_history_entry(&line).into_diagnostic()?;

                line
            }
            Err(ReadlineError::Interrupted) => {
                print!("\x1b[0m");
                std::io::stdout().flush().ok();
                break;
            }
            Err(ReadlineError::Eof) => {
                print!("\x1b[0m");
                std::io::stdout().flush().ok();
                break;
            }
            Err(err) => {
                shell.error(err).into_diagnostic()?;
                print!("\x1b[0m");
                std::io::stdout().flush().ok();
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
                    print!("\x1b[0m");
                    std::io::stdout().flush().ok();
                    std::process::exit(0);
                }
                ":list" => {
                    for (id, command) in bundle.definition().commands.iter() {
                        println!("{}: {}", id, command);
                    }
                    println!();
                }
                ":ast" => {
                    let json = serde_json::to_string_pretty(&**bundle.definition()).unwrap();
                    shell
                        .print_highlighted_stdout(&json, "json")
                        .into_diagnostic()?;
                    println!();
                }
                ":step" => {
                    let cur = is_stepping
                        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |v| Some(!v))
                        .unwrap();

                    if !cur {
                        shell.status("Stepping", "enabled").into_diagnostic()?;
                    } else {
                        shell.status("Stepping", "disabled").into_diagnostic()?;
                    }
                }
                ":config" => {
                    let json = serde_json::to_string_pretty(&config).unwrap();
                    shell
                        .print_highlighted_stdout(&json, "json")
                        .into_diagnostic()?;
                    println!();
                }
                ":save" => {
                    let filename = chunks.next().unwrap_or("pipeline_debug.md");
                    match save_markdown(&last_run, filename) {
                        Ok(()) => shell
                            .status("Saved", format!("Debug log to {}", filename))
                            .into_diagnostic()?,
                        Err(e) => shell
                            .error(format!("Failed to save: {}", e))
                            .into_diagnostic()?,
                    }
                }
                ":set" => {
                    let Some(var) = chunks.next() else {
                        shell.error("Missing id name").into_diagnostic()?;
                        continue;
                    };
                    let value = chunks.collect::<Vec<_>>().join(" ");
                    let value = match serde_json::from_str::<serde_json::Value>(&value) {
                        Ok(v) => v,
                        Err(e) => {
                            shell
                                .error(format!("Failed to parse value: {}", e))
                                .into_diagnostic()?;
                            continue;
                        }
                    };

                    shell
                        .status("Setting", format!("{var} = {value:?}"))
                        .into_diagnostic()?;
                    config
                        .as_object_mut()
                        .unwrap()
                        .insert(var.to_string(), value);
                    pipe = bundle
                        .create_with_tap(config.clone(), tap.clone())
                        .await
                        .into_diagnostic()?;
                }
                ":breakpoint" => {
                    let arg = chunks.next();
                    let mut breakpoint_guard = breakpoint.write().await;
                    match arg {
                        Some("clear") | None => {
                            *breakpoint_guard = None;
                            shell.status("Breakpoint", "cleared").into_diagnostic()?;
                        }
                        Some(id) => {
                            if bundle.definition().commands.contains_key(id) {
                                *breakpoint_guard = Some(id.to_string());
                                shell
                                    .status("Breakpoint", format!("set at command '{}'", id))
                                    .into_diagnostic()?;
                            } else {
                                shell
                                    .error(format!("Command '{}' not found", id))
                                    .into_diagnostic()?;
                                continue;
                            }
                        }
                    }
                }
                unknown => {
                    shell
                        .error(format!("Unknown command: {}", unknown))
                        .into_diagnostic()?;
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

        let output_cmd = bundle.definition().output.resolve(bundle.definition());

        while let Some(input) = stream.next().await {
            match input {
                Ok(input) => {
                    if let Some(ref colors) = output_cmd_colors {
                        // Bold green [result] with themed background, newlines before and after
                        println!(
                            "{}\x1b[1;32m\x1b[0m{}\x1b[K",
                            colors.background, colors.background
                        );
                        println!(
                            "{}\x1b[1;32m[result]\x1b[0m{}\x1b[K",
                            colors.background, colors.background
                        );
                    } else {
                        println!();
                        println!("\x1b[1;32m[result]\x1b[0m");
                    }
                    print_input_highlighted(shell, &input, output_cmd)?;

                    if let Some(path) = args.output_path.as_deref() {
                        match input {
                            Input::Multiple(_) => todo!("multiple not supported"),
                            Input::String(s) => std::fs::write(path, s).into_diagnostic()?,
                            Input::Bytes(b) => std::fs::write(path, b).into_diagnostic()?,
                            Input::Json(j) => std::fs::write(
                                path,
                                serde_json::to_string_pretty(&j).into_diagnostic()?,
                            )
                            .into_diagnostic()?,
                            Input::ArrayString(_) => todo!("multiple not supported"),
                            Input::ArrayBytes(_) => todo!("multiple not supported"),
                        }

                        if let Some(app) = args.command.as_deref() {
                            shell
                                .status_with_color(
                                    "Running",
                                    format!("{app} {}", path.display()),
                                    Color::Cyan,
                                )
                                .into_diagnostic()?;
                            if cfg!(windows) {
                                std::process::Command::new("pwsh")
                                    .arg("-c")
                                    .arg(format!("{app} {}", path.display()))
                                    .spawn()
                                    .unwrap()
                                    .wait()
                                    .into_diagnostic()?;
                            } else {
                                std::process::Command::new("sh")
                                    .arg("-c")
                                    .arg(format!("{app} {}", path.display()))
                                    .spawn()
                                    .unwrap()
                                    .wait()
                                    .into_diagnostic()?;
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

    // Reset terminal colors before exiting
    print!("\x1b[0m");
    std::io::stdout().flush().ok();

    rl.save_history(&history_path).into_diagnostic()?;

    Ok(())
}

fn parse_config(config: &[String]) -> miette::Result<serde_json::Value> {
    tracing::debug!("Parsing config: {:?}", config);
    let map = config
        .iter()
        .map(|x| {
            let mut arr = x.splitn(2, '=');
            let Some(a) = arr.next() else {
                miette::bail!("Invalid input: {}", x);
            };
            let Some(b) = arr.next() else {
                miette::bail!("Invalid input: {}", x);
            };
            serde_json::from_str::<'_, serde_json::Value>(b)
                .into_diagnostic()
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

fn save_markdown(last_run: &Arc<Mutex<Option<PipelineRun>>>, filename: &str) -> miette::Result<()> {
    use std::fmt::Write;

    let run = match last_run.lock() {
        Ok(run) => run.clone(),
        Err(_) => miette::bail!("Failed to acquire lock on pipeline run"),
    };

    let run = match run {
        Some(run) => run,
        None => miette::bail!("No pipeline run to export"),
    };

    let mut markdown = String::new();

    writeln!(markdown, "# Pipeline Debug Report").into_diagnostic()?;
    writeln!(markdown).into_diagnostic()?;
    writeln!(markdown, "## Input").into_diagnostic()?;
    writeln!(markdown, "```").into_diagnostic()?;
    writeln!(markdown, "{}", run.input).into_diagnostic()?;
    writeln!(markdown, "```").into_diagnostic()?;
    writeln!(markdown).into_diagnostic()?;
    writeln!(markdown, "## Pipeline Execution").into_diagnostic()?;
    writeln!(markdown).into_diagnostic()?;

    for event in &run.events {
        let command_str = strip_ansi_codes(&format!("{}", event.command));
        let event_str = strip_ansi_codes(&format!("{:#}", event.event));

        writeln!(markdown, "<details>").into_diagnostic()?;
        writeln!(
            markdown,
            "<summary><code>[{}]</code> <code>{}</code></summary>",
            event.key, command_str
        )
        .into_diagnostic()?;
        writeln!(markdown).into_diagnostic()?;
        writeln!(markdown, "```").into_diagnostic()?;
        writeln!(markdown, "{}", event_str).into_diagnostic()?;
        writeln!(markdown, "```").into_diagnostic()?;
        writeln!(markdown, "</details>").into_diagnostic()?;
        writeln!(markdown).into_diagnostic()?;
    }

    std::fs::write(filename, markdown).into_diagnostic()?;

    Ok(())
}

pub async fn run(shell: &mut Shell, mut args: RunArgs) -> miette::Result<()> {
    let path = args
        .path
        .as_ref()
        .cloned()
        .unwrap_or_else(|| std::env::current_dir().unwrap());
    let bundle = if path.extension().map(|x| x.as_encoded_bytes()) == Some(b"drb") {
        if let Some(ref pipeline_name) = args.pipeline {
            Bundle::from_bundle_named(&path, pipeline_name)
                .await
                .into_diagnostic()?
        } else {
            Bundle::from_bundle(&path).await.into_diagnostic()?
        }
    } else {
        // For TypeScript files, prepare the environment (sync + type check)
        let pipeline_path = if path.ends_with(".ts") {
            path.clone()
        } else {
            path.join("pipeline.ts")
        };

        if pipeline_path.exists() {
            utils::prepare_typescript_pipeline(shell, &pipeline_path, args.skip_check)?;
        }

        crate::deno_rt::save_ast(&path, "pipeline.json")?;
        if let Some(ref pipeline_name) = args.pipeline {
            Bundle::from_path_named(&path, pipeline_name)
                .await
                .into_diagnostic()?
        } else {
            Bundle::from_path(&path).await.into_diagnostic()?
        }
    };

    let config = parse_config(&args.config)?;

    if !std::io::stdin().is_terminal() {
        let mut s = String::new();
        std::io::stdin().read_to_string(&mut s).into_diagnostic()?;
        args.input = Some(s);
    }

    let mut pipe = bundle.create(config).await.into_diagnostic()?;

    if let Some(input) = args.input {
        let mut stream = pipe.forward(Input::String(input)).await;

        let output_cmd = bundle.definition().output.resolve(bundle.definition());

        while let Some(Ok(input)) = stream.next().await {
            print_input_highlighted(shell, &input, output_cmd)?;
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
