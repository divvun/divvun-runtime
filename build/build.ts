import { bold, cyan, yellow } from "jsr:@std/fmt@1/colors";
import { ensureDeps } from "./deps.ts";
import {
  BuildTool,
  buildToolToCommand,
  exec,
  getEnvVars,
  getHostTriple,
  getSysrootEnv,
  needsCrossCompile,
  stripBinary,
} from "./util.ts";

// Build library
export async function buildLib(
  target?: string,
  debug = false,
  verbose = 0,
) {
  const host = getHostTriple();
  const buildTool = needsCrossCompile(host, target);

  // Ensure dependencies are set up
  await ensureDeps(target);

  console.log(
    cyan(bold("Building")) +
      ` libdivvun_runtime ${
        debug ? yellow("DEBUG") : bold("release")
      } for target: ${bold(target || host)}` +
      (buildTool !== BuildTool.Cargo ? " " + yellow(`(${buildTool})`) : ""),
  );

  const baseCmd = buildToolToCommand(buildTool);
  const args = [...baseCmd, "build", "--features", "ffi"];

  if (!debug) {
    args.push("--release");
  }

  if (verbose > 0) {
    args.push("-" + "v".repeat(verbose));
  }

  if (target) {
    args.push("--target", target);
  }

  const env: Record<string, string> = Deno.env.toObject();

  if (target?.includes("ios")) {
    env["RUSTFLAGS"] = "-C link-arg=-Wl,-U,___chkstk_darwin";
  }

  // Add sysroot env vars if cross-compiling
  Object.assign(env, getEnvVars(target));
  if (buildTool !== BuildTool.Cargo && target) {
    Object.assign(env, getSysrootEnv(target));
  }

  await exec(args, env);
}

// Build CLI
export async function build(target?: string, debug = false, verbose = 0) {
  const host = getHostTriple();
  const buildTool = needsCrossCompile(host, target);

  // Ensure dependencies are set up
  await ensureDeps(target);

  console.log(
    cyan(bold("Building")) +
      ` CLI (${debug ? yellow("debug") : bold("release")}) for target: ${
        bold(target || host)
      }` +
      (buildTool !== BuildTool.Cargo ? " " + yellow(`(${buildTool})`) : ""),
  );

  const baseCmd = buildToolToCommand(buildTool);
  const args = [
    ...baseCmd,
    "build",
    "-p",
    "divvun-runtime-cli",
    "--features",
    "divvun-runtime/all-mods,ffi",
  ];

  if (!debug) {
    args.push("--release");
  }

  if (verbose > 0) {
    args.push("-" + "v".repeat(verbose));
  }

  if (target) {
    args.push("--target", target);
  }

  const env: Record<string, string> = Deno.env.toObject();

  if (target == null) {
    env["RUSTFLAGS"] = "-C target-cpu=native";
  } else if (target.includes("ios")) {
    env["RUSTFLAGS"] = "-C link-arg=-Wl,-U,___chkstk_darwin";
  }

  // Add sysroot env vars if cross-compiling
  Object.assign(env, getEnvVars(target));
  if (buildTool !== BuildTool.Cargo && target) {
    Object.assign(env, getSysrootEnv(target));
  }

  await exec(args, env);

  // Strip binary
  await stripBinary(target, debug);
}

// Check CLI
export async function check(target?: string, debug = false, verbose = 0) {
  const host = getHostTriple();
  const buildTool = needsCrossCompile(host, target);

  // Ensure dependencies are set up
  await ensureDeps(target);

  console.log(
    cyan(bold("Checking")) +
      ` CLI (${debug ? yellow("debug") : bold("release")}) for target: ${
        bold(target || host)
      }` +
      (buildTool !== BuildTool.Cargo ? " " + yellow(`(${buildTool})`) : ""),
  );

  const baseCmd = buildToolToCommand(buildTool);
  const args = [
    ...baseCmd,
    "check",
    "-p",
    "divvun-runtime-cli",
    "--features",
    "divvun-runtime/all-mods,ffi",
  ];

  if (!debug) {
    args.push("--release");
  }

  if (verbose > 0) {
    args.push("-" + "v".repeat(verbose));
  }

  if (target) {
    args.push("--target", target);
  }

  const env: Record<string, string> = Deno.env.toObject();

  if (target == null) {
    env["RUSTFLAGS"] = "-C target-cpu=native";
  } else if (target.includes("ios")) {
    env["RUSTFLAGS"] = "-C link-arg=-Wl,-U,___chkstk_darwin";
  }

  // Add sysroot env vars if cross-compiling
  Object.assign(env, getEnvVars(target));
  if (buildTool !== BuildTool.Cargo && target) {
    Object.assign(env, getSysrootEnv(target));
  }

  await exec(args, env);
}
