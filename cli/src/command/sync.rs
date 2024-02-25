use crate::{cli::SyncArgs, shell::Shell};

pub async fn sync(shell: &mut Shell, args: SyncArgs) -> anyhow::Result<()> {
    let cur_dir = args
        .path
        .unwrap_or_else(|| std::env::current_dir().unwrap());

    shell.status("Initializing", "Python virtual environment")?;
    let result = std::process::Command::new("python3")
        .args(&["-m", "venv"])
        .arg(&cur_dir.join(".venv"))
        .output()?;

    if !result.status.success() {
        shell.error("Failed to create virtual environment")?;
        std::process::exit(1);
    }

    shell.status("Installing", "Divvun Runtime Python bindings")?;
    let py_ver_path = std::fs::read_dir(cur_dir.join(".venv").join("lib"))?
        .next()
        .unwrap()
        .unwrap()
        .file_name();

    let site_packages_path = cur_dir
        .join(".venv")
        .join("lib")
        .join(py_ver_path)
        .join("site-packages")
        .join("divvun_runtime");

    match std::fs::remove_dir_all(&site_packages_path) {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => {
            shell.error(format!("Failed to remove site-packages directory: {}", e))?;
            std::process::exit(1);
        }
    }
    divvun_runtime::py::generate(&site_packages_path)?;

    Ok(())
}
