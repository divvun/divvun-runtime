use std::path::PathBuf;
use std::process::Command;
use walkdir::WalkDir;

use crate::{cli::TestArgs, shell::Shell};

fn collect_ts_files(path: &PathBuf) -> anyhow::Result<Vec<PathBuf>> {
    let mut files = Vec::new();

    if path.is_file() {
        if path.extension().and_then(|e| e.to_str()) == Some("ts") {
            files.push(path.clone());
        }
    } else if path.is_dir() {
        for entry in WalkDir::new(path)
            .follow_links(false)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let entry_path = entry.path();
            if entry_path.is_file() && entry_path.extension().and_then(|e| e.to_str()) == Some("ts")
            {
                files.push(entry_path.to_path_buf());
            }
        }
    }

    Ok(files)
}

pub async fn test(_shell: &mut Shell, args: TestArgs) -> anyhow::Result<()> {
    let exe_path = std::env::current_exe()?;

    let mut test_files = Vec::new();

    if args.files.is_empty() {
        let default_tests_dir = PathBuf::from("tests");
        if default_tests_dir.exists() && default_tests_dir.is_dir() {
            test_files = collect_ts_files(&default_tests_dir)?;
        }

        if test_files.is_empty() {
            anyhow::bail!("No test files found. Either create a 'tests/' directory with .ts files, or specify test files/directories as arguments.");
        }
    } else {
        for path in &args.files {
            let mut collected = collect_ts_files(path)?;
            test_files.append(&mut collected);
        }

        if test_files.is_empty() {
            anyhow::bail!("No .ts test files found in the specified paths.");
        }
    }

    test_files.sort();

    let mut cmd = Command::new("deno");
    cmd.arg("test")
        .arg("--hide-stacktraces")
        .arg("--parallel")
        .arg("--allow-ffi")
        .arg("--allow-env")
        .arg("--no-check")
        .env("LIB_PATH", exe_path);

    for file in &test_files {
        cmd.arg(file);
    }

    if !args.script_args.is_empty() {
        cmd.arg("--");
        for arg in &args.script_args {
            cmd.arg(arg);
        }
    }

    let status = cmd.status()?;

    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }

    Ok(())
}
