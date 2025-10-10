# Installation

This guide covers installing Divvun Runtime on macOS and Linux.

## Prerequisites

### All Platforms

- **Rust toolchain**: Install from [rustup.rs](https://rustup.rs/)
- **`just` command runner**: `cargo install just`

### macOS

```bash
# Install PyTorch (required for speech features)
brew install pytorch

# Install ICU4C (required for text processing)
brew install icu4c
```

### Linux

```bash
# Install libtorch 2.4.1+ with C++11 ABI
# Download from https://pytorch.org/
# Extract to /opt/libtorch

# Install ICU development libraries
# Ubuntu/Debian:
sudo apt-get install libicu-dev

# Fedora/RHEL:
sudo dnf install icu
```

## Building from Source

### Clone the Repository

```bash
git clone https://github.com/divvun/divvun-runtime.git
cd divvun-runtime
```

### Build the CLI

```bash
# Build CLI
just build

# Install to ~/.cargo/bin
just install
```

The CLI will be available as `divvun-runtime`.

### Build the UI (Optional)

```bash
# Build UI
just build-ui

# Or run in development mode
just run-ui
```

### Verify Installation

```bash
divvun-runtime --version
```

You should see output like:
```
divvun-runtime-cli 0.1.0
```

## Next Steps

Now that Divvun Runtime is installed, continue to the [Quick Start](./quick-start.md) guide to create your first pipeline.
