tmp := `mktemp -d`
pwd := `pwd`

build-lib-ios-aarch64:
    LIBTORCH_LIB=/Users/brendan/git/divvun/divvun-speech-rs/contrib/build/ios/pytorch/LibTorchLite.xcframework/ios-arm64 \
        LIBTORCH_INCLUDE=/Users/brendan/git/divvun/divvun-speech-rs/contrib/build/ios/pytorch/LibTorchLite.xcframework/ios-arm64/Headers \
        LIBTORCH=/Users/brendan/git/divvun/divvun-speech-rs/contrib/build/ios/pytorch/LibTorchLite.xcframework/ios-arm64 \
        LIBTORCH_LITE=1 LIBTORCH_STATIC=1 \
        cargo build --lib --release --no-default-features --features mod-speech,ffi --target aarch64-apple-ios

build-cli-linux:
    @ARTIFACT_PATH=/usr \
        LZMA_API_STATIC=1 \
        TMP_PATH={{tmp}} \
        LIBTORCH=/usr/local \
        LIBTORCH_BYPASS_VERSION_CHECK=1 \
        cargo build -p divvun-runtime-cli --no-default-features --release \
        --features divvun-runtime/mod-cg3,divvun-runtime/mod-divvun,divvun-runtime/mod-speech
    

build-cli-macos:
    @# Workaround for macOS eagerly linking dylibs no matter what we tell it
    mkdir -p {{tmp}}/lib
    ln -s /opt/homebrew/opt/icu4c/lib/*.a {{tmp}}/lib
    ln -s /opt/libtorch/lib/*.a {{tmp}}/lib
    ARTIFACT_PATH=/opt/homebrew/opt/python@3.11/Frameworks/Python.framework/Versions/3.11 \
        LZMA_API_STATIC=1 \
        TMP_PATH={{tmp}} \
        PYO3_CONFIG_FILE={{pwd}}/pyo3-mac.txt \
        LIBTORCH=/opt/libtorch \
        LIBTORCH_BYPASS_VERSION_CHECK=1 \
        cargo build -p divvun-runtime-cli --no-default-features --release \
        --features divvun-runtime/mod-cg3,divvun-runtime/mod-hfst,divvun-runtime/mod-divvun,divvun-runtime/mod-speech
    install_name_tool -add_rpath /opt/libtorch/lib ./target/release/divvun-runtime
    rm -rf {{tmp}}

install-cli-macos: build-cli-macos
    install -m 755 ./target/release/divvun-runtime $HOME/.cargo/bin/divvun-runtime
