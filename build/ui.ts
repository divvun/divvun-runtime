import { bold, cyan, yellow } from "jsr:@std/fmt@1/colors";
import { ensureDeps } from "./deps.ts";
import { exec, getEnvVars, getSysrootEnv } from "./util.ts";

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
      ` UI (${
        debug ? yellow("debug") : bold("release")
      }) for ${platform} target: ${bold(target || "host")}`,
  );

  // Change to playground directory and install dependencies
  Deno.chdir("playground");
  await exec(["pnpm", "i"]);

  // For iOS, generate .xcconfig with environment variables
  if (platform === "ios") {
    const env = { ...getEnvVars(target) };
    if (target) {
      Object.assign(env, getSysrootEnv(target));
    }

    // Add APPLE_DEVELOPMENT_TEAM from environment
    const appleTeam = Deno.env.get("APPLE_DEVELOPMENT_TEAM");
    if (appleTeam) {
      env["APPLE_DEVELOPMENT_TEAM"] = appleTeam;
    }

    const xcconfigPath = "src-tauri/gen/apple/build.xcconfig";
    const xcconfigContent = Object.entries(env)
      .map(([key, value]) => `${key} = "${value}"`)
      .join("\n");

    await Deno.writeTextFile(xcconfigPath, xcconfigContent);
  }

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
      if (Deno.build.os === "darwin") {
        buildArgs.push("--bundles", "app");
      } else {
        buildArgs.push("--no-bundle");
      }
      break;
  }

  if (debug) {
    buildArgs.push("--debug");
  }

  // Build environment
  const env = { ...Deno.env.toObject(), ...getEnvVars(target) };
  if (target) {
    Object.assign(env, getSysrootEnv(target));
  }

  await exec(buildArgs, env);
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
