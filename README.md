[![CI](https://builds.giellalt.org/api/badge/divvun-runtime)](https://builds.giellalt.org/pipelines/divvun-runtime)

 Divvun Runtime

Modular language processing pipeline system for building grammar checkers and text-to-speech applications.

Define pipelines in TypeScript, compile to efficient Rust runtime, distribute as single-file bundles.

## Features

- **TypeScript Pipelines**: Define text processing workflows in TypeScript
- **Modular Architecture**: Composable modules for HFST, CG3, spell checking, and TTS
- **Bundle Distribution**: Package pipelines with assets into `.drb` files
- **Cross-platform**: Rust core with Swift/Java/Deno/Python bindings and CLI tools

## Quick Start

```bash
# Install dependencies
brew install pytorch icu4c  # macOS
cargo install just

# Clone and build
git clone https://github.com/divvun/divvun-runtime.git
cd divvun-runtime
just build
just install

# Create a pipeline
divvun-runtime init
divvun-runtime run ./pipeline.ts "Hello World"
```

## Use Cases

### Grammar Checking
Build spell and grammar checkers with contextual error detection and suggestions.

### Text-to-Speech
Create TTS systems with phonological processing and normalization.

## Documentation

Full documentation: https://divvun.github.io/divvun-runtime/

- [Installation Guide](https://divvun.github.io/divvun-runtime/installation/)
- [Quick Start](https://divvun.github.io/divvun-runtime/quick-start/)
- [Grammar Checking Tutorial](https://divvun.github.io/divvun-runtime/grammar/overview/)
- [Text-to-Speech Tutorial](https://divvun.github.io/divvun-runtime/tts/overview/)

## Building

```bash
# Build CLI
just build

# Install to ~/.cargo/bin
just install

# Build UI (optional)
just build-ui

# Run UI in dev mode
just run-ui
```

## License

The **divvun-runtime library** is dual-licensed under:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

You may choose either license for library use.

The **command-line tools and playground** (`cli/`, `playground/`) are licensed under **GPL-3.0** ([LICENSE-GPL](LICENSE-GPL)).