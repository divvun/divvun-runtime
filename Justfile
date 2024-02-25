cpython-dist := "/Users/brendan/git/necessary/python-build-standalone/dist/cpython-3.11.7-aarch64-apple-darwin-pgo-20240206T1427.tar.zst"
cpython-dist-sha256 := "359639d1ecaddb05a9361822e6cf9dc227b5dbb123e6d573566fff4dd80945d3"

tmp := `mktemp -d`

build-cli:
    @pyoxidizer generate-python-embedding-artifacts --system-rust --dynamic \
        {{tmp}} {{cpython-dist}} {{cpython-dist-sha256}}
    @ARTIFACT_PATH={{tmp}} PYO3_CONFIG_FILE={{tmp}}/pyo3-build-config-file.txt \
        cargo build -p divvun-runtime-cli
    @install_name_tool -change /install/lib/libpython3.11.dylib @executable_path/libpython3.11.dylib ./target/debug/divvun-runtime-cli
    @rm -r {{tmp}}