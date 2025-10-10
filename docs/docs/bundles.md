# Bundles

Bundles (`.drb` files) package pipelines with their assets for distribution.

## Creating a Bundle

```bash
divvun-runtime bundle [OPTIONS]
```

Options:
- `-a, --assets-path <PATH>` - Assets directory (default: `./assets`)
- `-p, --pipeline-path <PATH>` - Pipeline file (default: `./pipeline.ts`)
- `--skip-check` - Skip TypeScript type checking

Example:
```bash
cd my-project
divvun-runtime bundle
```

Creates `bundle.drb` with all pipelines and assets.

## Bundle Contents

A bundle contains:

- Compiled pipeline code
- Assets directory (models, data files)
- Metadata (pipeline names, default pipeline)

Pipelines ending in `_dev` are automatically excluded from bundles. See [Pipelines](./pipelines.md#dev-pipelines) for details.

## Using Bundles

### Run a Bundle

```bash
divvun-runtime run bundle.drb "input text"
```

### List Pipelines

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

### Select Pipeline

```bash
divvun-runtime run --pipeline spell-only bundle.drb "text"
```

## Distribution

Distribute the `.drb` file:

- Single file contains everything
- No external dependencies
- Cross-platform compatible

Users run without installing project:
```bash
divvun-runtime run your-bundle.drb "input"
```
