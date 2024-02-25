use box_format::{BoxFileWriter, BoxPath, Compression};

use crate::{cli::BundleArgs, shell::Shell};

pub fn bundle(shell: &mut Shell, args: BundleArgs) -> anyhow::Result<()> {
    let mut box_file = BoxFileWriter::create_with_alignment("./bundle.drb", 8)?;
    box_file.insert(
        Compression::Stored,
        BoxPath::new("pipeline.py").unwrap(),
        &mut std::fs::File::open("./pipeline.py")?,
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
