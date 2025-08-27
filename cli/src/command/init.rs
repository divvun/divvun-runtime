use crate::{
    cli::{InitArgs, SyncArgs},
    shell::Shell,
};

use super::sync::sync;

pub async fn init(shell: &mut Shell, args: InitArgs) -> anyhow::Result<()> {
    sync(
        shell,
        SyncArgs {
            path: args.path.clone(),
        },
    )
    .await?;

    let cur_dir = args
        .path
        .unwrap_or_else(|| std::env::current_dir().unwrap());

    shell.status("Creating", "pipeline.ts")?;

    std::fs::write(cur_dir.join("pipeline.ts"), EXAMPLE_TS)?;

    Ok(())
}

const EXAMPLE_TS: &str = r#"import { Command, StringEntry } from './.divvun-rt/mod.ts';
import * as example from './.divvun-rt/example.ts';

// Run `divvun-runtime run ./pipeline.ts "This is some input"` to see what it does.

export default function examplePipeline(entry: StringEntry): Command {
    let x = example.reverse(entry);
    x = example.upper(x);

    return x;
}
"#;
