set unstable

env_vars := if os() == "linux" {
    "LZMA_API_STATIC=1 LIBTORCH_BYPASS_VERSION_CHECK=1 LIBTORCH=/usr"
} else if os() == "macos" {
    "LZMA_API_STATIC=1 LIBTORCH=/opt/homebrew"
} else if os() == "windows" {
    "LZMA_API_STATIC=1 LIBTORCH=\"C:\\libtorch\""
} else {
    ""
}

build target="":
    @echo "Building for target: {{target}}"
    {{env_vars}} cargo build -p divvun-runtime-cli --features divvun-runtime/all-mods --release {{ if target != "" { "--target" } else { "" } }} {{ target }}

# Install built binary
install target="": (build target)
    @echo "Installing divvun-runtime for target: {{target}}"
    {{ if os() == "windows" { "copy .\\target\\" + target + "\\release\\divvun-runtime.exe %USERPROFILE%\\.cargo\\bin\\divvun-runtime.exe" } else { "rm -f ~/.cargo/bin/divvun-runtime && cp ./target/" + target + "/release/divvun-runtime ~/.cargo/bin/divvun-runtime" } }}

# Print inventory of registered modules and structs
print-inventory:
    {{env_vars}} cargo run --example print_inventory --features all-mods