# Installation

This guide covers installing Divvun Runtime from a release or building it from
source.

## Deno

Install [Deno](https://deno.com) first (`brew install deno` on macOS). The CLI
uses it for TypeScript pipelines: `init`, `sync`, `bundle`, `test`, and `run` on
a `.ts` file. Running a `.drb` bundle does not need Deno.

## Prebuilt Binaries

CLI binaries and libraries are attached to the
[GitHub releases](https://github.com/divvun/divvun-runtime/releases). The
`dev-latest` pre-release is built from `main`.

Download the `divvun-runtime` archive for your platform, extract it, and put the
`divvun-runtime` binary on your `PATH`. The `libdivvun_runtime` archives hold
the library for embedding through the C ABI and the bindings.

## Building from Source

### Prerequisites

- A stable Rust toolchain from [rustup.rs](https://rustup.rs/) (the crates use
  edition 2024)
- Deno, which runs the `./x` build tool
- CMake and a C/C++ compiler, for the XNNPACK library that speech synthesis
  builds
- On Linux, `clang` and `lld`

```bash
# macOS (the Xcode command line tools provide the compiler)
xcode-select --install
brew install cmake

# Debian/Ubuntu
sudo apt install clang lld cmake

# Fedora
sudo dnf install clang lld cmake
```

CG-3, HFST and the speech engine are pure Rust, so there is no ICU, libtorch or
other native library to install.

### Clone the Repository

```bash
git clone https://github.com/divvun/divvun-runtime.git
cd divvun-runtime
```

### Build the CLI

```bash
# Build the release CLI with every module
./x build

# Build, then install to ~/.cargo/bin
./x install
```

Use `./x.ps1` on Windows. `./x build` tunes the binary for the CPU it is built
on. Pass `--target <triple>` to cross-compile, or `--debug` for a debug build.

`./x` wraps cargo, so plain cargo works too:

```bash
cargo build -p divvun-runtime-cli --release --features divvun-runtime/all-mods
```

The CLI crate only enables the grammar checking modules by default. Pass
`divvun-runtime/all-mods` to include speech, SSML and jq.

### Build the UI (Optional)

The UI is a Tauri app with a Preact frontend built through Deno. It additionally
requires the Tauri CLI (`cargo install tauri-cli`).

```bash
# Build UI
./x build-ui

# Or run in development mode
./x run-ui
```

`divvun-runtime playground` opens a bundle in it.

### Verify Installation

```bash
divvun-runtime --version
```

You should see output like:
```
Divvun Runtime v0.4.0 (2026-10-29, b45299b)
```

## Next Steps

Now that Divvun Runtime is installed, continue to the [Quick Start](./quick-start.md) guide to create your first pipeline.
