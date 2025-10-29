import { bold, cyan, yellow } from "jsr:@std/fmt@1/colors";
import { ensureDeps } from "./deps.ts";
import { exec, getEnvVars } from "./util.ts";

// Build Tauri UI
export async function buildUi(target?: string, debug = false) {
  // Ensure dependencies are set up
  await ensureDeps(target);

  console.log(
    cyan(bold("Building")) +
      ` UI (${debug ? yellow("debug") : bold("release")}) for target: ${
        bold(target || "host")
      }`,
  );

  // Update cargo deps in tauri
  await exec(["cargo", "update"], {});
  Deno.chdir("playground/src-tauri");
  Deno.chdir("../..");

  // Build with pnpm
  Deno.chdir("playground");
  await exec(["pnpm", "i"]);

  const buildArgs = ["pnpm", "tauri", "build", "--bundles", "app"];
  if (debug) {
    buildArgs.push("--debug");
  }

  await exec(buildArgs, getEnvVars(target));
  Deno.chdir("..");
}

// Run Tauri UI in dev mode
export async function runUi() {
  console.log(cyan(bold("Running")) + " UI in dev mode");

  Deno.chdir("playground");
  await exec(["pnpm", "i"]);
  await exec(["pnpm", "tauri", "dev"], getEnvVars());
  Deno.chdir("..");
}
