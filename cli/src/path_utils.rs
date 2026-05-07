use std::path::Path;

pub fn is_ts_path(path: &Path) -> bool {
    path.extension().map(|x| x.as_encoded_bytes()) == Some(b"ts")
}

#[cfg(test)]
mod tests {
    use super::is_ts_path;
    use std::path::Path;

    #[test]
    fn detects_ts_file_extension() {
        assert!(is_ts_path(Path::new("pipeline.ts")));
    }

    #[test]
    fn rejects_non_ts_extension() {
        assert!(!is_ts_path(Path::new("pipeline.json")));
    }

    #[test]
    fn rejects_path_with_ts_directory_name() {
        assert!(!is_ts_path(Path::new("tools/pipeline.ts/pipeline")));
    }

    #[test]
    fn detects_ts_with_parent_dirs() {
        assert!(is_ts_path(Path::new("tools/grammarcheckers/pipeline.ts")));
    }
}
