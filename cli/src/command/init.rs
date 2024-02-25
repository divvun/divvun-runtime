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

    shell.status("Creating", "example.py")?;

    std::fs::write(cur_dir.join("example.py"), EXAMPLE_PY)?;

    Ok(())
}

const EXAMPLE_PY: &str = r#"from divvun_runtime import StringEntry, pipeline, example

# Run `divvun-runtime run ./example.py "This is some input"` to see what it does.

@pipeline
def example_pipeline(entry: StringEntry):
    x = example.reverse(entry)
    x = example.upper(x)

    return x

"#;
