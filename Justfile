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
    
    LZMA_API_STATIC=1 \
        LIBTORCH=/usr/local \
        RUSTFLAGS="-C link-arg=-Wl,-rpath,/usr/local/lib" \
        $builder build -p divvun-runtime-cli --no-default-features --release \
        --features divvun-runtime/all-mods --target $target

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
    
    LZMA_API_STATIC=1 \
        LIBTORCH=/opt/homebrew \
        cargo build -p divvun-runtime-cli --release \
        --target $target \
        --features divvun-runtime/all-mods

[script]
install-cli-linux arch="x86_64": build-cli-linux
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
    install -m 755 ./target/$target/release/divvun-runtime $HOME/.cargo/bin/divvun-runtime

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
