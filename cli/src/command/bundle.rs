use box_format::{BoxFileWriter, BoxPath, Compression};

use crate::{cli::BundleArgs, py_rt::{self, init_py}, shell::Shell};

pub fn bundle(shell: &mut Shell, args: BundleArgs) -> anyhow::Result<()> {
    init_py();

    let value = crate::py_rt::dump_ast(&std::fs::read_to_string("./pipeline.py")?)?;

    let mut box_file = BoxFileWriter::create_with_alignment("./bundle.drb", 8)?;
    box_file.insert(
        Compression::Stored,
        BoxPath::new("pipeline.json").unwrap(),
        &mut std::io::Cursor::new(serde_json::to_vec(&value)?),
        Default::default(),
    )?;

    let maybe_assets = match std::fs::read_dir("./assets") {
        Ok(v) => Some(v),
        Err(x) if x.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => {
            shell.error(format!("Failed to read assets directory: {}", e))?;
            std::process::exit(1);
        }
    };

    if let Some(assets) = maybe_assets {
        for entry in assets.filter_map(Result::ok) {
            if !entry.file_type()?.is_file() {
                continue;
            }
            box_file.insert(
                Compression::Stored,
                BoxPath::new(&entry.file_name())?,
                &mut std::fs::File::open(&entry.path())?,
                Default::default(),
            )?;
        }
    }

    box_file.finish()?;

    Ok(())
}
