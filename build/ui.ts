import { bold, cyan, yellow } from "jsr:@std/fmt@1/colors";
import { ensureDeps } from "./deps.ts";
import { exec, getEnvVars } from "./util.ts";

// Build Tauri UI
export async function buildUi(target?: string, debug = false) {
  // Ensure dependencies are set up
  await ensureDeps(target);

  // Detect platform from target
  let platform: "ios" | "android" | "desktop" = "desktop";
  if (target?.includes("ios")) {
    platform = "ios";
  } else if (target?.includes("android")) {
    platform = "android";
  }

  console.log(
    cyan(bold("Building")) +
      ` UI (${debug ? yellow("debug") : bold("release")}) for ${platform} target: ${
        bold(target || "host")
      }`,
  );

  // Change to playground directory and install dependencies
  Deno.chdir("playground");
  await exec(["pnpm", "i"]);

  // Build command based on platform
  const buildArgs = ["pnpm", "tauri"];

  switch (platform) {
    case "ios":
      buildArgs.push("ios", "build");
      break;
    case "android":
      buildArgs.push("android", "build");
      break;
    case "desktop":
      buildArgs.push("build");
      break;
  }

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
