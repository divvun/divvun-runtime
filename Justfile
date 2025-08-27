set unstable

# build-cli-linux [arch]
# Builds the CLI for Linux. Supports x86_64 (default) and aarch64.
[script]
build-cli-linux arch="x86_64":
    case {{arch}} in
        "aarch64")
            target="aarch64-unknown-linux-gnu"
            builder="cross"
            ;;
        *)
            target="x86_64-unknown-linux-gnu"
            builder="cargo"
            ;;
    esac
    tmp=`mktemp -d`
    ARTIFACT_PATH=/usr \
        LZMA_API_STATIC=1 \
        TMP_PATH=$tmp \
        LIBTORCH=/usr/local \
        LIBTORCH_BYPASS_VERSION_CHECK=1 \
        $builder build -p divvun-runtime-cli --no-default-features --release \
        --features divvun-runtime/all-mods --target $target
    rm -rf $tmp

# build-cli-macos [arch]
# Builds the CLI for macOS. Supports x86_64 (default) and aarch64.
# Note: This requires the `icu4c` and `libtorch` libraries to be installed via Homebrew.
[script]
build-cli-macos arch="aarch64":
    case {{arch}} in
        "aarch64")
            builder="cargo";
            target="aarch64-apple-darwin";
            ;;
        *)
            builder="cross";
            target="x86_64-apple-darwin";
            ;;
    esac
    
    tmp=`mktemp -d`

    # Workaround for macOS eagerly linking dylibs no matter what we tell it
    mkdir -p $tmp/lib
    ln -s /opt/homebrew/opt/icu4c/lib/*.a $tmp/lib
    ln -s /opt/libtorch/lib/*.a $tmp/lib
    ARTIFACT_PATH=/opt/homebrew/opt/python@3.11/Frameworks/Python.framework/Versions/3.11 \
        LZMA_API_STATIC=1 \
        TMP_PATH=$tmp \
        PYO3_CONFIG_FILE=`pwd`/pyo3-mac.txt \
        LIBTORCH=/opt/libtorch \
        LIBTORCH_BYPASS_VERSION_CHECK=1 \
        cargo build -p divvun-runtime-cli --release \
        --target $target \
        --features divvun-runtime/all-mods
    install_name_tool -add_rpath /opt/libtorch/lib ./target/$target/release/divvun-runtime
    rm -rf $tmp

[script]
test:
    tmp=`mktemp -d`
    
    mkdir -p $tmp/lib
    ln -s /opt/homebrew/opt/icu4c/lib/*.a $tmp/lib
    ln -s /opt/libtorch/lib/*.a $tmp/lib
    ARTIFACT_PATH=/opt/homebrew/opt/python@3.11/Frameworks/Python.framework/Versions/3.11 \
        LZMA_API_STATIC=1 \
        TMP_PATH=$tmp \
        PYO3_CONFIG_FILE=`pwd`/pyo3-mac.txt \
        LIBTORCH=/opt/libtorch \
        LIBTORCH_BYPASS_VERSION_CHECK=1 \
        cargo test lol --lib --no-default-features --features mod-cg3
    rm -rf $tmp

[script]
install-cli-macos arch="aarch64": build-cli-macos
    case {{arch}} in
        "aarch64")
            target="aarch64-apple-darwin";
            ;;
        *)
            target="x86_64-apple-darwin";
            ;;
    esac
    install -m 755 ./target/$target/release/divvun-runtime $HOME/.cargo/bin/divvun-runtime
