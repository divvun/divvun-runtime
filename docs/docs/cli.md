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

Automatically excludes dev pipelines (functions ending in `_dev`).

## run

Execute a pipeline.

```bash
divvun-runtime run [OPTIONS] <path> [input]
```

**Path**: Can be `.drb` bundle, `.ts` pipeline file, or directory

**Options**:
- `-p, --path <PATH>` - Alternative way to specify path
- `-P, --pipeline <NAME>` - Select specific pipeline
- `-c, --config <KEY=VALUE>` - Runtime configuration
- `-o, --output-path <PATH>` - Write output to file
- `-C, --command <CMD>` - Run command on output
- `--skip-check` - Skip type checking

**Examples**:
```bash
# Run from TypeScript
divvun-runtime run ./pipeline.ts "text"

# Run specific pipeline
divvun-runtime run --pipeline spell-only ./pipeline.ts "text"

# Run from bundle
divvun-runtime run bundle.drb "text"

# With configuration
divvun-runtime run -c 'suggest={"locales":["fo"]}' bundle.drb "text"

# Save output
divvun-runtime run -o output.wav bundle.drb "text"
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
