import { text } from "jsr:@optique/core@0.6.2/message";
import {
  command,
  constant,
  flag,
  merge,
  object,
  option,
  optional,
  or,
  Parser,
} from "jsr:@optique/core@0.6.2/parser";
import { string } from "jsr:@optique/core@0.6.2/valueparser";
import { run } from "jsr:@optique/run@0.6.2";
import { bold, green, red } from "jsr:@std/fmt@1/colors";
import { build, buildLib } from "./build.ts";
import { setupDeps } from "./deps.ts";
import { install } from "./install.ts";
import { buildUi, runUi } from "./ui.ts";

// Common options for commands
const targetOption = object({
  target: optional(option("--target", string({ metavar: "TRIPLE" }))),
});

const debugOption = object({
  debug: optional(flag("--debug")),
});

const buildOptions = merge(targetOption, debugOption);

enum Subcommand {
  BuildLib = "build-lib",
  Build = "build",
  Install = "install",
  BuildUi = "build-ui",
  RunUi = "run-ui",
  Deps = "deps",
}

const subcommand = <T, S>(
  name: Subcommand,
  description: string,
  options?: Parser<T, S>,
) => {
  return command(
    name,
    merge(options ?? object({}), object({ command: constant(name) })),
    {
      description: [text(description)],
    },
  );
};

const buildLibCommand = subcommand(
  Subcommand.BuildLib,
  "Build divvun-runtime library",
  buildOptions,
);

const buildCommand = subcommand(
  Subcommand.Build,
  "Build CLI binary (default: release)",
  buildOptions,
);

const installCommand = subcommand(
  Subcommand.Install,
  "Install CLI binary",
  buildOptions,
);

const buildUiCommand = subcommand(
  Subcommand.BuildUi,
  "Build Tauri playground UI",
  buildOptions,
);

const runUiCommand = subcommand(
  Subcommand.RunUi,
  "Run Tauri playground in dev mode",
  object({}),
);

const depsCommand = subcommand(
  Subcommand.Deps,
  "Setup dependencies (download and link static libs)",
  targetOption,
);

// Main CLI parser
const parser = or(
  buildLibCommand,
  buildCommand,
  installCommand,
  buildUiCommand,
  runUiCommand,
  depsCommand,
);

const VERSION = (() => {
  const p = `${import.meta.filename?.split('/').slice(0, -1).join("/")}/../Cargo.toml`
  const data = new TextDecoder().decode(Deno.readFileSync(p));
  const r= /version = "([^"]+)"/.exec(data);
  return r ? r[1] : "unknown";
})()

const config = run(parser, {
  help: "both",
  showDefault: true,
  brief: [text(bold(green(`Divvun Runtime Build Tool v${VERSION}\n`)))],
  programName: Deno.build.os === "windows" ? "./x.ps1" : "./x",
  aboveError: "help",
});

switch (config.command) {
  case Subcommand.BuildLib:
    await buildLib(
      "target" in config ? config.target : undefined,
      "debug" in config ? config.debug : false,
    );
    break;
  case Subcommand.Build:
    await build(
      "target" in config ? config.target : undefined,
      "debug" in config ? config.debug : false,
    );
    break;
  case Subcommand.Install:
    await install(
      "target" in config ? config.target : undefined,
      "debug" in config ? config.debug : false,
    );
    break;
  case Subcommand.BuildUi:
    await buildUi(
      "target" in config ? config.target : undefined,
      "debug" in config ? config.debug : false,
    );
    break;
  case Subcommand.RunUi:
    await runUi();
    break;
  case Subcommand.Deps:
    await setupDeps("target" in config ? config.target : undefined);
    break;
  default:
    console.error(red("Error: Unknown command"));
    Deno.exit(1);
}
