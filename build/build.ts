import { bold, cyan, yellow } from "jsr:@std/fmt@1/colors";
import {
  exec,
  getEnvVars,
  getHostTriple,
  getSysrootEnv,
  needsCross,
  stripBinary,
} from "./util.ts";

// Build library
export async function buildLib(target?: string, debug = false) {
  const host = getHostTriple();
  const useCross = needsCross(host, target);

  console.log(
    cyan(bold("Building")) +
      ` libdivvun_runtime ${debug ? yellow("DEBUG") : bold("release")} for target: ${
        bold(target || host)
      }` +
      (useCross ? " " + yellow("(cross)") : ""),
  );

  const command = useCross ? "cross" : "cargo";
  const args = [command, "build", "--features", "ffi"];

  if (!debug) {
    args.push("--release");
  }

  if (target) {
    args.push("--target", target);
  }

  // Add sysroot env vars if cross-compiling
  const env = { ...getEnvVars(target) };
  if (useCross && target) {
    Object.assign(env, getSysrootEnv(target));
  }

  await exec(args, env);
}

// Build CLI
export async function build(target?: string, debug = false) {
  const host = getHostTriple();
  const useCross = needsCross(host, target);

  console.log(
    cyan(bold("Building")) +
      ` CLI (${debug ? yellow("debug") : bold("release")}) for target: ${
        bold(target || host)
      }` +
      (useCross ? " " + yellow("(cross)") : ""),
  );

  const command = useCross ? "cross" : "cargo";
  const args = [
    command,
    "build",
    "-p",
    "divvun-runtime-cli",
    "--features",
    "divvun-runtime/all-mods,ffi",
  ];

  if (!debug) {
    args.push("--release");
  }

  if (target) {
    args.push("--target", target);
  }

  const env: Record<string, string> = {};
  if (target == null) {
    env["RUSTFLAGS"] = "-C target-cpu=native";
  }

  // Add sysroot env vars if cross-compiling
  Object.assign(env, getEnvVars(target));
  if (useCross && target) {
    Object.assign(env, getSysrootEnv(target));
  }

  await exec(args, env);

  // Strip binary
  await stripBinary(target, debug);
}
