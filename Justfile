set unstable

env_vars := if os() == "linux" {
    "LZMA_API_STATIC=1 LIBTORCH_TOOLCHAIN=llvm LIBTORCH_BYPASS_VERSION_CHECK=1 LIBTORCH=/home/brendan/pytorch-static-build/target/x86_64-unknown-linux-gnu LIBTORCH_STATIC=1"
} else if os() == "macos" {
    "LZMA_API_STATIC=1 LIBTORCH=/opt/homebrew"
} else if os() == "windows" {
    "LZMA_API_STATIC=1 LIBTORCH=\"C:\\libtorch\""
} else {
    ""
}

build-lib target="":
    @echo "Building libdivvun_runtime for target: {{target}}"
    {{env_vars}} cargo build --features ffi --release {{ if target != "" { "--target" } else { "" } }} {{ target }}

build target="":
    @echo "Building for target: {{target}}"
    {{env_vars}} cargo build -vv -p divvun-runtime-cli --features divvun-runtime/all-mods,ffi --release {{ if target != "" { "--target" } else { "" } }} {{ target }}
    strip -x -S ./target/{{ if target != "" { target + "/" } else { "" } }}release/divvun-runtime

# Install built binary
install target="": (build target)
    @echo "Installing divvun-runtime for target: {{target}}"
    {{ if os() == "windows" { "copy .\\target\\" + target + "\\release\\divvun-runtime.exe %USERPROFILE%\\.cargo\\bin\\divvun-runtime.exe" } else { "rm -f ~/.cargo/bin/divvun-runtime && cp ./target/" + target + "/release/divvun-runtime ~/.cargo/bin/divvun-runtime" } }}

build-ui target="": 
    @echo "Building UI for target: {{target}}"
    cd playground/src-tauri && cargo update
    cd playground && pnpm i && {{env_vars}} pnpm tauri build --bundles app

run-ui:
    @echo "Running UI"
    cd playground && pnpm i && {{env_vars}} pnpm tauri dev

# Print inventory of registered modules and structs
print-inventory:
    {{env_vars}} cargo run --example print_inventory --features all-mods
