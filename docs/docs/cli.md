# CLI Reference

All `divvun-runtime` commands.

## init

Initialize a new pipeline project.

```bash
divvun-runtime init [path]
```

Creates `pipeline.ts` and generates type definitions in `.divvun-rt/`.

## sync

Generate TypeScript type definitions.

```bash
divvun-runtime sync [path]
```

Run after changing Cargo features or updating Divvun Runtime.

## bundle

Create a `.drb` bundle for distribution.

```bash
divvun-runtime bundle [OPTIONS]
```

**Options**:
- `-a, --assets-path <PATH>` - Assets directory (default: `./assets`)
- `-p, --pipeline-path <PATH>` - Pipeline file (default: `./pipeline.ts`)
- `--skip-check` - Skip TypeScript type checking
- `--type <TYPE>` - Bundle type metadata (e.g. `grammar`, `spellcheck`, `tts`)
- `--name <NAME>` - Bundle name metadata
- `--vers <VERSION>` - Bundle version metadata
- `--locales <TAGS>` - BCP-47 tags the bundle serves, comma-separated (e.g.
  `se-NO,se-SE`), recorded as `drb.locales`

Writes `bundle.drb` in the current directory. Dev pipelines (functions ending
in `_dev`) are left out.

## run

Execute a pipeline.

```bash
divvun-runtime run [OPTIONS] [input]
```

**Path**: `-p, --path <PATH>` takes a `.drb` bundle, a `.ts` pipeline file, or
a directory containing `pipeline.ts`. It defaults to the current directory.

**Input**: the text to process. With no input, `run` starts an interactive
REPL. When stdin is not a terminal, `run` reads its input from stdin instead,
and the `input` argument is ignored.

**Options**:
- `-p, --path <PATH>` - Bundle, pipeline file, or directory to run
- `-P, --pipeline <NAME>` - Select a specific named pipeline
- `-c, --config <KEY=VALUE>` - Runtime configuration
- `-o, --output-path <PATH>` - Write output to file
- `-C, --command <CMD>` - Run command on output
- `-b, --break-after <STEP>` - Run up to the named step, print its raw output,
  and stop. A step is named by the id passed as a command's first argument;
  an unknown name lists the available steps.
- `--skip-check` - Skip type checking

**Examples**:
```bash
# Run pipeline.ts in the current directory
divvun-runtime run "text"

# Run a TypeScript pipeline
divvun-runtime run -p ./pipeline.ts "text"

# Run specific pipeline
divvun-runtime run -p ./pipeline.ts --pipeline spell-only "text"

# Run from bundle
divvun-runtime run -p bundle.drb "text"

# Read input from stdin
echo "text" | divvun-runtime run -p bundle.drb

# Inspect an intermediate step
divvun-runtime run -p bundle.drb --break-after <step> "text"

# With configuration
divvun-runtime run -p bundle.drb -c 'suggest={"locales":["fo"]}' "text"

# Save output
divvun-runtime run -p bundle.drb -o output.wav "text"
```

## list

List pipelines in a bundle or project.

```bash
divvun-runtime list <path>
```

Shows all available pipelines and marks default and dev pipelines.

**Example**:
```bash
divvun-runtime list bundle.drb
```

Output:
```
Bundle bundle.drb
Pipelines 2 available

• grammar-checker (default)
• spell-only
```

## playground

Open a bundle or pipeline in the graphical playground (alias `play`).

```bash
divvun-runtime playground [path]
```

Looks for the playground app in `$DRT_PLAYGROUND`, then
`/Applications/Divvun Runtime Playground.app` on macOS, then
`divvun-rt-playground` on `PATH`.

## test

Run TypeScript test files with Deno.

```bash
divvun-runtime test [files...] [-- script args]
```

With no files, runs the `.ts` files in `tests/`.

## Global options

- `-V, --version` - Show the version, build date and git revision. `-VV` adds
  the enabled features, dependencies and toolchain.
- `--theme <THEME>` - Syntax highlighting theme (also `$DRT_THEME`)

## Configuration Syntax

Runtime configuration passed with `-c` flag:

**String**:
```bash
-c 'cmd-id="value"'
```

**JSON object**:
```bash
-c 'cmd-id={"key":"value","num":42}'
```

**Array**:
```bash
-c 'cmd-id=["val1","val2"]'
```

**Multiple configs**:
```bash
-c 'cmd1="val1"' -c 'cmd2={"key":"val2"}'
```

Common configurations:
```bash
# Locale selection
-c 'suggest={"locales":["fo","en"]}'

# Ignore error types
-c 'suggest={"ignore":["typo"]}'

# UTF-16 encoding
-c 'suggest={"encoding":"utf-16"}'

# TTS speaker override
-c 'tts-cmd={"speaker":1}'
```
