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
import * as path from "jsr:@std/path@1";
import { build, buildLib, check, doc, test } from "./build.ts";
import { setupDeps } from "./deps.ts";
import { install } from "./install.ts";
import { buildUi, runUi } from "./ui.ts";
import { assertHostToolchain } from "./util.ts";

// Common options for commands
const targetOption = object({
  target: optional(option("--target", string({ metavar: "TRIPLE" }))),
});

const debugOption = object({
  debug: optional(flag("--debug")),
});

const verboseOption = object({
  v: optional(flag("-v")),
  vv: optional(flag("-vv")),
  vvv: optional(flag("-vvv")),
  verbose: optional(flag("--verbose")),
});

const buildOptions = merge(targetOption, merge(debugOption, verboseOption));

enum Subcommand {
  BuildLib = "build-lib",
  Build = "build",
  Check = "check",
  Test = "test",
  Doc = "doc",
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

const checkCommand = subcommand(
  Subcommand.Check,
  "Check CLI without building",
  buildOptions,
);

const testCommand = subcommand(
  Subcommand.Test,
  "Run cargo tests for divvun-runtime",
  buildOptions,
);

const docCommand = subcommand(
  Subcommand.Doc,
  "Generate rustdoc HTML (cargo doc; output in target/doc)",
  object({}),
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
  checkCommand,
  testCommand,
  docCommand,
  installCommand,
  buildUiCommand,
  runUiCommand,
  depsCommand,
);

const VERSION = (() => {
  const p = path.join(import.meta.dirname ?? "", "..", "Cargo.toml");
  const data = new TextDecoder().decode(Deno.readFileSync(p));
  const r = /version = "([^"]+)"/.exec(data);
  return r ? r[1] : "unknown";
})();

const config = run(parser, {
  help: "both",
  showDefault: true,
  brief: [text(bold(green(`Divvun Runtime Build Tool v${VERSION}\n`)))],
  programName: Deno.build.os === "windows" ? "./x.ps1" : "./x",
  aboveError: "help",
});

function verboseLevel(c: unknown): number {
  const o = c as { v?: boolean; vv?: boolean; vvv?: boolean; verbose?: boolean };
  if (o.vvv) return 3;
  if (o.vv) return 2;
  if (o.v || o.verbose) return 1;
  return 0;
}

switch (config.command) {
  case Subcommand.BuildLib:
    await assertHostToolchain();
    await buildLib(
      "target" in config ? config.target : undefined,
      "debug" in config ? config.debug : false,
      verboseLevel(config),
    );
    break;
  case Subcommand.Build:
    await assertHostToolchain();
    await build(
      "target" in config ? config.target : undefined,
      "debug" in config ? config.debug : false,
      verboseLevel(config),
    );
    break;
  case Subcommand.Check:
    await assertHostToolchain();
    await check(
      "target" in config ? config.target : undefined,
      "debug" in config ? config.debug : false,
      verboseLevel(config),
    );
    break;
  case Subcommand.Test:
    await assertHostToolchain();
    await test(
      "target" in config ? config.target : undefined,
      "debug" in config ? config.debug : false,
      verboseLevel(config),
    );
    break;
  case Subcommand.Doc:
    await assertHostToolchain();
    await doc();
    break;
  case Subcommand.Install:
    await assertHostToolchain();
    await install(
      "target" in config ? config.target : undefined,
      "debug" in config ? config.debug : false,
    );
    break;
  case Subcommand.BuildUi:
    await assertHostToolchain();
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
