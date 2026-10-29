[![CI](https://builds.giellalt.org/api/badge/divvun-runtime)](https://builds.giellalt.org/pipelines/divvun-runtime)

# Divvun Runtime

Modular language processing pipeline system for building spell checkers,
grammar checkers and text-to-speech applications.

Define pipelines in TypeScript, run them on a Rust runtime, and distribute them
as single-file `.drb` bundles.

## Features

- **TypeScript pipelines**: describe a pipeline as a graph of commands in
  TypeScript; the runtime executes it natively
- **Modules**: HFST tokenisation, VISL CG-3, spelling, grammar error
  suggestions, text-to-speech, SSML and jq, each behind a cargo feature
- **Bundles**: package a pipeline and its assets into one `.drb` file, with
  models memory-mapped straight out of the bundle
- **Embedding**: C ABI with Swift, Java, Python and Deno bindings in
  [`bindings/`](bindings)
- **Playground**: a desktop app for running a bundle and inspecting the output
  of every step

The runtime is pure Rust. CG-3 and HFST are native Rust ports
([cg3-rs](https://github.com/divvun/cg3-rs),
[hfst-rs](https://github.com/divvun/hfst-rs)), and speech synthesis runs on
[executorch-rs](https://github.com/divvun/executorch-rs). The only C code left
is the XNNPACK library that executorch-rs compiles for `mod-speech`.

## Installation

Prebuilt CLI binaries and libraries are attached to the
[GitHub releases](https://github.com/divvun/divvun-runtime/releases). The
`dev-latest` pre-release is built from `main`.

Install [Deno](https://deno.com) as well (`brew install deno` on macOS). The CLI
needs it for TypeScript pipelines: `init`, `sync`, `bundle`, `test`, and `run`
on a `.ts` file. Running a `.drb` bundle does not need Deno.

To build from source instead, see [Building](#building).

## Quick Start

```bash
# Create pipeline.ts and the generated TypeScript API in .divvun-rt/
divvun-runtime init

# Run the pipeline in the current directory
divvun-runtime run "Hello World"

# Or name the pipeline file explicitly
divvun-runtime run -p ./pipeline.ts "Hello World"

# With no input, `run` starts a REPL. Piped stdin replaces the INPUT argument.
echo "Hello World" | divvun-runtime run

# Package pipeline.ts and ./assets into bundle.drb
divvun-runtime bundle

# Run a bundle, stopping after one step to see its raw output
divvun-runtime run -p bundle.drb --break-after <step> "Some text"
```

## CLI

| Command      | Purpose                                                       |
| ------------ | ------------------------------------------------------------- |
| `init`       | Create a pipeline project with an example `pipeline.ts`       |
| `sync`       | Regenerate the TypeScript API in `.divvun-rt/`                |
| `run`        | Run a pipeline (`.ts` file, directory, or `.drb` bundle)      |
| `bundle`     | Package a pipeline and its assets into a `.drb` bundle        |
| `list`       | List the pipelines and metadata in a bundle                   |
| `playground` | Open a bundle in the playground (alias `play`)                |
| `test`       | Run TypeScript test files with Deno                           |

Useful `run` options: `-P <name>` selects a named pipeline, `-c` passes
pipeline config, `-o` writes output to a file, and `--break-after <step>` stops
after a step and prints its output. See `divvun-runtime <command> --help`.

Pipeline functions whose names end in `_dev` are development pipelines. They
can load files relative to `pipeline.ts` with `@`-prefixed paths and are left
out of bundles.

## Modules

| Module   | Feature      | Commands                                                   |
| -------- | ------------ | ---------------------------------------------------------- |
| `hfst`   | `mod-hfst`   | `tokenize`                                                 |
| `cg3`    | `mod-cg3`    | `vislcg3`, `mwesplit`, `sentences`, `streamcmd`, `to_json` |
| `divvun` | `mod-divvun` | `blanktag`, `cgspell`, `suggest`                           |
| `speech` | `mod-speech` | `normalize`, `phon`, `tts`                                 |
| `ssml`   | `mod-ssml`   | `strip`                                                    |
| `jq`     | `mod-jq`     | `jq`                                                       |

`spell`, `runtime` and `debug` are always available. `all-mods` enables every
module.

## Building

You need:

- a stable Rust toolchain (the crates use edition 2024)
- [Deno](https://deno.com), which runs the `./x` build tool
- CMake and a C/C++ compiler, for XNNPACK under `mod-speech`
- on Linux, `clang` and `lld` 19 or newer (`sudo apt install clang lld`)

Bison, flex, ICU and libtorch are no longer needed.

```bash
git clone https://github.com/divvun/divvun-runtime.git
cd divvun-runtime

./x build      # release CLI with every module, tuned for this CPU
./x install    # build, then copy to ~/.cargo/bin/divvun-runtime
./x test       # cargo tests for divvun-runtime
./x build-lib  # static and dynamic libraries for the bindings
./x build-ui   # build the playground
./x run-ui     # run the playground in dev mode
```

Use `./x.ps1` on Windows. Pass `--debug` for a debug build and `--target
<triple>` to cross-compile. Run `./x --help` to list every command.

`./x` only wraps cargo, so plain cargo works too:

```bash
cargo build -p divvun-runtime-cli --release --features divvun-runtime/all-mods
cargo test -p divvun-runtime --features all-mods,ffi
```

The CLI crate only enables `mod-divvun` by default, so pass
`divvun-runtime/all-mods` to get speech, SSML and jq. See
[`playground/README.md`](playground/README.md) for working on the playground.

## Documentation

Full documentation: https://divvun.github.io/divvun-runtime/

- [Quick Start](https://divvun.github.io/divvun-runtime/quick-start/)
- [Pipelines](https://divvun.github.io/divvun-runtime/pipelines/)
- [Modules & Commands](https://divvun.github.io/divvun-runtime/modules/)
- [Grammar Checking Tutorial](https://divvun.github.io/divvun-runtime/grammar/overview/)
- [Text-to-Speech Tutorial](https://divvun.github.io/divvun-runtime/tts/overview/)

## License

The **divvun-runtime library** is dual-licensed under:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

You may choose either license for library use.

The **command-line tools and playground** (`cli/`, `playground/`) are licensed under **GPL-3.0** ([LICENSE-GPL](LICENSE-GPL)).
