#cpython-dist := "/Users/brendan/git/necessary/python-build-standalone/dist/cpython-3.11.7-aarch64-apple-darwin-pgo-20240206T1427.tar.zst"
#cpython-dist-sha256 := "359639d1ecaddb05a9361822e6cf9dc227b5dbb123e6d573566fff4dd80945d3"

cpython-dist := "/Users/brendan/Downloads/cpython-3.11.7-aarch64-unknown-linux-gnu-pgo-20240225T1814.tar.zst"
cpython-dist-sha256 := "bb3caad7d7970aac5377153070932eea429a5919dadd11e2b4b4f05869bd62df"

tmp := `mktemp -d`

build-cli:
    @echo {{tmp}}
    @pyoxidizer generate-python-embedding-artifacts --system-rust \
        {{tmp}} {{cpython-dist}} {{cpython-dist-sha256}}
    @sed -ie 's/\.so.*//g' {{tmp}}/pyo3-build-config-file.txt
    @ARTIFACT_PATH={{tmp}} PYO3_CONFIG_FILE={{tmp}}/pyo3-build-config-file.txt \
        cargo build -p divvun-runtime-cli --no-default-features --features divvun-runtime/mod-cg3,divvun-runtime/mod-hfst,divvun-runtime/mod-divvun
    #@install_name_tool -change /install/lib/libpython3.11.dylib @executable_path/libpython3.11.dylib ./target/debug/divvun-runtime-cli
    #@rm -r {{tmp}}
