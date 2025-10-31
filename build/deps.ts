import { MultiProgressBar } from "jsr:@deno-library/progress";
import { bold, cyan, dim, green, red, yellow } from "jsr:@std/fmt@1/colors";
import { getHostTriple } from "./util.ts";

// Hardcoded dependency versions
const DEPS = {
  icu4c: "77.1",
  pytorch: "2.8.0",
  protobuf: "33.0",
  libomp: "21.1.4",
} as const;

const GITHUB_REPO = "divvun/static-lib-build";

// Check if target is Windows
function isWindows(target: string): boolean {
  return target.includes("windows");
}

function isIOS(target: string): boolean {
  return target.includes("ios");
}

// Progress state for a single package
interface ProgressState {
  completed: number;
  total: number;
  text: string;
}

// Create a transform stream to track download progress
function createProgressStream(
  bars: MultiProgressBar,
  progressState: Map<string, ProgressState>,
  packageName: string,
): TransformStream<Uint8Array, Uint8Array> {
  return new TransformStream({
    async transform(chunk, controller) {
      const state = progressState.get(packageName);
      if (state) {
        state.completed += chunk.byteLength;
        await bars.render(Array.from(progressState.values()));
      }
      controller.enqueue(chunk);
    },
    async flush() {
      // Ensure progress bar shows 100% complete
      const state = progressState.get(packageName);
      if (state) {
        state.completed = state.total;
        await bars.render(Array.from(progressState.values()));
      }
    },
  });
}

// Get platform-specific dependencies
function getPlatformDeps(
  target: string,
): Record<string, string | null> {
  if (isWindows(target)) {
    // Windows only needs icu4c and pytorch
    return {
      ...DEPS,
      protobuf: null,
      libomp: null,
    };
  }

  if (isIOS(target)) {
    return {
      ...DEPS,
      libomp: null,
    };
  }

  // Linux/macOS need all dependencies
  return { ...DEPS };
}

// Download and extract a package
async function downloadPackage(
  name: string,
  version: string,
  target: string,
  bars: MultiProgressBar,
  progressState: Map<string, ProgressState>,
): Promise<void> {
  const packagesDir = `.x/packages/${target}/${name}`;

  // Create packages directory
  await Deno.mkdir(packagesDir, { recursive: true });

  // Special case: Windows pytorch comes from pytorch.org
  if (name === "pytorch" && isWindows(target)) {
    const url =
      `https://download.pytorch.org/libtorch/cpu/libtorch-win-shared-with-deps-${version}%2Bcpu.zip`;
    const filename = `libtorch-win-shared-with-deps-${version}+cpu.zip`;
    const zipPath = `.x/packages/${target}/${filename}`;

    // Download zip
    const response = await fetch(url);
    if (!response.ok) {
      throw new Error(
        `Failed to download ${name}: ${response.status} ${response.statusText}\nURL: ${url}`,
      );
    }

    // Initialize progress state
    const totalBytes = parseInt(
      response.headers.get("content-length") || "0",
      10,
    );
    progressState.set(name, {
      completed: 0,
      total: totalBytes,
      text: `${name} v${version}`,
    });

    const progressStream = createProgressStream(bars, progressState, name);

    const file = await Deno.open(zipPath, {
      create: true,
      write: true,
      truncate: true,
    });

    if (response.body) {
      await response.body
        .pipeThrough(progressStream)
        .pipeTo(file.writable);
    }

    // Extract zip to temp directory (pytorch.org zips have libtorch/ prefix)
    const tempDir = await Deno.makeTempDir();
    const unzip = Deno.build.os === "windows"
      ? new Deno.Command("powershell", {
        args: [
          "-Command",
          `Expand-Archive -Path "${zipPath}" -DestinationPath "${tempDir}" -Force`,
        ],
        stdout: "inherit",
        stderr: "inherit",
      })
      : new Deno.Command("unzip", {
        args: ["-q", "-o", zipPath, "-d", tempDir],
        stdout: "inherit",
        stderr: "inherit",
      });

    const { code } = await unzip.output();
    if (code !== 0) {
      throw new Error(`Failed to extract ${name}`);
    }

    // Move libtorch/* to packagesDir (flatten the structure)
    const libtorchDir = `${tempDir}/libtorch`;
    await Deno.rename(libtorchDir, packagesDir);

    // Clean up temp directory and zip
    await Deno.remove(tempDir, { recursive: true });
    await Deno.remove(zipPath);
    return;
  }

  // Standard case: Download from GitHub
  // URL pattern: https://github.com/divvun/static-lib-build/releases/download/{name}%2Fv{version}/{name}_v{version}_{target}.tar.gz
  const tag = `${name}%2Fv${version}`; // URL-encoded {name}/v{version}
  const filename = `${name}_v${version}_${target}.tar.gz`;
  const url =
    `https://github.com/${GITHUB_REPO}/releases/download/${tag}/${filename}`;

  // Download tarball
  const response = await fetch(url);
  if (!response.ok) {
    throw new Error(
      `Failed to download ${name}: ${response.status} ${response.statusText}\nURL: ${url}`,
    );
  }

  // Initialize progress state
  const totalBytes = parseInt(
    response.headers.get("content-length") || "0",
    10,
  );
  progressState.set(name, {
    completed: 0,
    total: totalBytes,
    text: `${name} v${version}`,
  });

  const progressStream = createProgressStream(bars, progressState, name);

  const tarballPath = `.x/packages/${target}/${filename}`;
  const file = await Deno.open(tarballPath, {
    create: true,
    write: true,
    truncate: true,
  });

  if (response.body) {
    await response.body
      .pipeThrough(progressStream)
      .pipeTo(file.writable);
  }

  // Extract tarball
  const tar = new Deno.Command("tar", {
    args: ["-xzf", tarballPath, "-C", packagesDir, "--strip-components=1"],
    stdout: "inherit",
    stderr: "inherit",
  });

  const { code } = await tar.output();
  if (code !== 0) {
    throw new Error(`Failed to extract ${name}`);
  }

  // Remove tarball
  await Deno.remove(tarballPath);
}

// Link package files into sysroot
async function linkPackage(name: string, target: string): Promise<void> {
  const packageDir = `.x/packages/${target}/${name}`;
  const sysrootDir = `.x/sysroot/${target}`;

  console.log(
    cyan("Linking") + ` ${bold(name)} into ${dim(sysrootDir)}`,
  );

  // Ensure sysroot directories exist
  await Deno.mkdir(`${sysrootDir}/lib`, { recursive: true });
  await Deno.mkdir(`${sysrootDir}/bin`, { recursive: true });
  await Deno.mkdir(`${sysrootDir}/include`, { recursive: true });

  // Link directories
  for (const dir of ["lib", "bin", "include"]) {
    const srcDir = `${packageDir}/${dir}`;
    const destDir = `${sysrootDir}/${dir}`;

    // Check if source directory exists
    try {
      const stat = await Deno.stat(srcDir);
      if (!stat.isDirectory) continue;
    } catch {
      continue; // Directory doesn't exist, skip
    }

    // Link/copy each file in the directory
    for await (const entry of Deno.readDir(srcDir)) {
      const srcPath = Deno.realPathSync(`${srcDir}/${entry.name}`);
      const destPath = `${destDir}/${entry.name}`;

      // Remove existing symlink/file if present
      try {
        await Deno.remove(destPath);
      } catch {
        // Ignore if doesn't exist
      }

      // Create symlink (or copy on Windows)
      if (Deno.build.os === "windows") {
        if (entry.isFile) {
          await Deno.copyFile(srcPath, destPath);
        } else if (entry.isDirectory) {
          // Recursively copy directory on Windows
          await copyDir(srcPath, destPath);
        }
      } else {
        await Deno.symlink(srcPath, destPath, {
          type: entry.isDirectory ? "dir" : "file",
        });
      }
    }
  }

  console.log(green("✓") + ` ${bold(name)} linked`);
}

// Recursively copy directory (for Windows)
async function copyDir(src: string, dest: string): Promise<void> {
  await Deno.mkdir(dest, { recursive: true });

  for await (const entry of Deno.readDir(src)) {
    const srcPath = `${src}/${entry.name}`;
    const destPath = `${dest}/${entry.name}`;

    if (entry.isFile) {
      await Deno.copyFile(srcPath, destPath);
    } else if (entry.isDirectory) {
      await copyDir(srcPath, destPath);
    }
  }
}

// Write manifest file with dependency versions
async function writeManifest(
  target: string,
  versions: Record<string, string | null>,
): Promise<void> {
  const sysrootDir = `.x/sysroot/${target}`;
  const manifestPath = `${sysrootDir}/manifest.json`;

  await Deno.mkdir(sysrootDir, { recursive: true });
  await Deno.writeTextFile(manifestPath, JSON.stringify(versions, null, 2));
}

// Read manifest file with dependency versions
async function readManifest(
  target: string,
): Promise<Record<string, string | null> | null> {
  const sysrootDir = `.x/sysroot/${target}`;
  const manifestPath = `${sysrootDir}/manifest.json`;

  try {
    const content = await Deno.readTextFile(manifestPath);
    return JSON.parse(content);
  } catch {
    return null;
  }
}

// Setup all dependencies for a target
export async function setupDeps(target?: string): Promise<void> {
  const actualTarget = target || getHostTriple();
  const platformDeps = getPlatformDeps(actualTarget);

  console.log(
    "\n" + cyan(bold("Setting up dependencies")) +
      ` for ${bold(actualTarget)}\n`,
  );

  // Create multi-progress bar for parallel downloads
  const bars = new MultiProgressBar({
    title: "Downloading dependencies",
    complete: "=",
    incomplete: "-",
    display: "[:bar] :text :percent :eta :completed/:total",
    prettyTime: true,
  });

  const progressState = new Map<string, ProgressState>();

  // Download and extract platform-specific packages in parallel
  const downloadResults = await Promise.allSettled(
    Object.entries(platformDeps).map(([name, version]) => {
      if (version !== null) {
        return downloadPackage(name, version, actualTarget, bars, progressState)
          .then(() => ({ name, version }));
      }
      return Promise.resolve({ name, version: null });
    }),
  );

  await bars.end();
  console.log(); // Empty line

  // Track successful and failed downloads
  const succeeded = new Set<string>();
  const failed: Array<{ name: string; error: string }> = [];

  downloadResults.forEach((result, index) => {
    const [name] = Object.entries(platformDeps)[index];
    if (result.status === "fulfilled" && result.value.version !== null) {
      succeeded.add(name);
    } else if (result.status === "rejected") {
      failed.push({
        name,
        error: result.reason?.message || String(result.reason),
      });
    }
  });

  // Report any failures and exit immediately
  if (failed.length > 0) {
    console.log(red(bold("✗ Failed to download some packages:")) + "\n");
    for (const { name, error } of failed) {
      console.log(red(`  • ${name}: ${error}`));
    }
    console.log();
    Deno.exit(1);
  }

  // Link successfully downloaded packages
  for (const name of succeeded) {
    await linkPackage(name, actualTarget);
  }

  // Write manifest with successful packages
  const successfulDeps: Record<string, string | null> = {};
  for (const [name, version] of Object.entries(platformDeps)) {
    if (version === null || succeeded.has(name)) {
      successfulDeps[name] = version;
    }
  }
  await writeManifest(actualTarget, successfulDeps);

  console.log(
    "\n" + green(bold("✓ Dependencies ready")) + ` for ${bold(actualTarget)}`,
  );
}

// Check if dependencies are set up for a target with correct versions
export async function depsExist(target?: string): Promise<boolean> {
  const actualTarget = target || getHostTriple();
  const platformDeps = getPlatformDeps(actualTarget);

  // Read manifest to check versions
  const manifest = await readManifest(actualTarget);
  if (!manifest) {
    return false;
  }

  // Check that all platform-specific dependencies exist with correct versions
  for (const [name, version] of Object.entries(platformDeps)) {
    if (manifest[name] !== version) {
      return false;
    }
  }

  return true;
}

// Ensure dependencies are set up (setup if needed)
export async function ensureDeps(target?: string): Promise<void> {
  if (!(await depsExist(target))) {
    console.log(
      yellow("⚠ Dependencies not found, setting up...") + "\n",
    );
    await setupDeps(target);
  }
}
