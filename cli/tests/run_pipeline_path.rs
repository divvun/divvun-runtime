use std::io::Write;
use std::process::{Command, Stdio};

#[test]
fn run_writes_pipeline_json_next_to_pipeline_not_working_directory() {
    #[cfg(feature = "ffi")]
    divvun_runtime::ffi::link_keep();

    let temp = tempfile::tempdir().unwrap();
    let workdir = temp.path().join("work");
    let pipeline_dir = temp.path().join("pipeline");
    std::fs::create_dir(&workdir).unwrap();
    std::fs::create_dir(&pipeline_dir).unwrap();
    std::fs::write(workdir.join("pipeline.json"), "do not overwrite").unwrap();
    std::fs::write(
        pipeline_dir.join("pipeline.ts"),
        r#"
import { StringEntry } from "./.divvun-rt/mod.ts";
import * as example from "./.divvun-rt/example.ts";
export default function reverse(entry: StringEntry) {
    return example.reverse("reverse", entry);
}
"#,
    )
    .unwrap();

    for path in [&pipeline_dir, &pipeline_dir.join("pipeline.ts")] {
        let mut child = Command::new(env!("CARGO_BIN_EXE_divvun-runtime"))
            .current_dir(&workdir)
            .args(["run", "-p"])
            .arg(path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child.stdin.take().unwrap().write_all(b"luohti").unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "run failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains("ithoul"));
        assert!(pipeline_dir.join("pipeline.json").is_file());
        assert_eq!(
            std::fs::read_to_string(workdir.join("pipeline.json")).unwrap(),
            "do not overwrite"
        );
    }
}
