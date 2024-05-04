macos-cpython-dist := "/Users/brendan/git/necessary/python-build-standalone/dist/cpython-3.11.7-aarch64-apple-darwin-pgo-20240206T1427.tar.zst"
macos-cpython-dist-sha256 := "359639d1ecaddb05a9361822e6cf9dc227b5dbb123e6d573566fff4dd80945d3"

linux-cpython-dist := "/Users/brendan/Downloads/cpython-3.11.7-aarch64-unknown-linux-gnu-pgo-20240225T1814.tar.zst"
linux-cpython-dist-sha256 := "bb3caad7d7970aac5377153070932eea429a5919dadd11e2b4b4f05869bd62df"

tmp := `mktemp -d`
pwd := `pwd`

build-cli-linux:
    @pyoxidizer generate-python-embedding-artifacts --system-rust \
        {{tmp}} {{linux-cpython-dist}} {{linux-cpython-dist-sha256}}
    @sed -ie 's/\.so.*//g' {{tmp}}/pyo3-build-config-file.txt
    @ARTIFACT_PATH={{tmp}} PYO3_CONFIG_FILE={{tmp}}/pyo3-build-config-file.txt \
        cargo build -p divvun-runtime-cli --no-default-features --release \
        --features divvun-runtime/mod-cg3,divvun-runtime/mod-hfst,divvun-runtime/mod-divvun
    @rm -r {{tmp}}

build-cli-macos:
    # Workaround for macOS eagerly linking dylibs no matter what we tell it
    mkdir -p {{tmp}}/lib
    cp -r /opt/homebrew/opt/icu4c/lib/*.a {{tmp}}/lib
    @ARTIFACT_PATH=/opt/homebrew/opt/python@3.11/Frameworks/Python.framework/Versions/3.11 \
        LZMA_API_STATIC=1 \
        TMP_PATH={{tmp}} \
        PYO3_CONFIG_FILE={{pwd}}/pyo3-mac.txt \
        cargo build -p divvun-runtime-cli --no-default-features --release \
        --features divvun-runtime/mod-cg3,divvun-runtime/mod-hfst,divvun-runtime/mod-divvun
    @install_name_tool -change /opt/homebrew/opt/python@3.11/Frameworks/Python.framework/Versions/3.11/Python @loader_path/libpython3.11.dylib ./target/release/divvun-runtime-cli
    cp /opt/homebrew/opt/python@3.11/Frameworks/Python.framework/Versions/3.11/Python ./target/release/libpython3.11.dylib
    @rm -rf {{tmp}}

build-lib-macos-aarch64:
    # Workaround for macOS eagerly linking dylibs no matter what we tell it
    mkdir -p {{tmp}}/lib
    cp -r /opt/homebrew/opt/icu4c/lib/*.a {{tmp}}/lib
    @ARTIFACT_PATH=/opt/homebrew/opt/python@3.11/Frameworks/Python.framework/Versions/3.11 \
        TMP_PATH={{tmp}} \
        LZMA_API_STATIC=1 \
        PYO3_CONFIG_FILE={{pwd}}/pyo3-mac.txt \
        cargo build -vv -p divvun-runtime --lib --no-default-features --release \
        --features ffi,divvun-runtime/mod-cg3,divvun-runtime/mod-hfst,divvun-runtime/mod-divvun
    @install_name_tool \
        -change /opt/homebrew/opt/python@3.11/Frameworks/Python.framework/Versions/3.11/Python \
        @loader_path/libpython3.11.dylib \
        ./target/release/libdivvun_runtime.dylib
    @rm -rf {{tmp}}

build-lib-macos-swift-aarch64:
    @CARGO_PROFILE_RELEASE_BUILD_OVERRIDE_DEBUG=true RUST_BACKTRACE=1 ARTIFACT_PATH=/opt/homebrew/opt/python@3.11/Frameworks/Python.framework/Versions/3.11 \
        PYO3_CONFIG_FILE={{pwd}}/pyo3-mac.txt \
        cargo build -p divvun-runtime --lib --no-default-features --features swift \
        --target aarch64-apple-darwin --release \
        --features divvun-runtime/mod-cg3,divvun-runtime/mod-hfst,divvun-runtime/mod-divvun -vv
    # swift-bridge-cli create-package \
    #     --bridges-dir ./generated \
    #     --out-dir DivvunRuntime \
    #     --macos target/aarch64-apple-darwin/release/libdivvun_runtime.a \
    #     --name DivvunRuntime
