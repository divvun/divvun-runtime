import { bold, cyan, dim, green } from "jsr:@std/fmt@1/colors";
import { build } from "./build.ts";

// Install binary
export async function install(target?: string, debug = false) {
  await build(target, debug);

  console.log(
    cyan("Installing") + ` divvun-runtime for target: ${bold(target || "host")}`,
  );

  const buildType = debug ? "debug" : "release";
  const targetPath = target ? `${target}/` : "";
  const sourcePath = `./target/${targetPath}${buildType}/divvun-runtime`;
  const destPath = `${Deno.env.get("HOME")}/.cargo/bin/divvun-runtime`;

  // Remove existing binary
  try {
    await Deno.remove(destPath);
  } catch {
    // Ignore if doesn't exist
  }

  // Copy new binary
  await Deno.copyFile(sourcePath, destPath);
  console.log(green("✓ Installed to ") + dim(destPath));
}
