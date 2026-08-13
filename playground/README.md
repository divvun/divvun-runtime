# Divvun Runtime Playground

A Tauri desktop app for loading `.drb` bundles, running pipelines interactively, and
inspecting the output of each command in the pipeline.

The frontend is Preact + Vite, run entirely through Deno. There is no `package.json` —
dependencies and tasks live in `deno.json`, and `node`/`npm`/`pnpm` are not required.

## Prerequisites

- [Deno](https://deno.com) 2.x — drives the frontend build
- Rust toolchain
- Tauri CLI: `cargo install tauri-cli`

## Development

```bash
# Run the app in dev mode (Vite dev server + native shell, with HMR)
deno task dev

# Build a release bundle
deno task build
```

From the repository root, `./x run-ui` and `./x build-ui` wrap these and additionally
handle cross-compilation targets and iOS `.xcconfig` generation.

## Frontend-only tasks

Useful when working on the UI without rebuilding the Rust side:

```bash
deno task vite        # dev server alone on http://localhost:1420
deno task vite:build  # typecheck + production build into dist/
deno task preview     # serve the built dist/
deno check            # typecheck only
```

`deno task dev` and `deno task build` invoke `deno task vite` / `deno task vite:build`
themselves via `beforeDevCommand` / `beforeBuildCommand` in
`src-tauri/tauri.conf.json` — you don't need to run them separately.

## Notes

- Relative imports need explicit `.ts` / `.tsx` extensions, as Deno requires.
- `src/App.css` is linked from `index.html` rather than imported from TypeScript, so
  that `deno check` can type-check the source graph. Vite still bundles it and
  hot-reloads it.
- Deno manages `node_modules/` itself (`nodeModulesDir: "auto"`), which Vite needs.
  Don't run a package manager against it.

## Recommended IDE Setup

- [VS Code](https://code.visualstudio.com/) +
  [Tauri](https://marketplace.visualstudio.com/items?itemName=tauri-apps.tauri-vscode) +
  [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer) +
  [Deno](https://marketplace.visualstudio.com/items?itemName=denoland.vscode-deno)
