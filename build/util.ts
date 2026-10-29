import { cyan, dim, red } from "jsr:@std/fmt@1/colors";

let toolchainChecked = false;

async function runs(bin: string, args: string[]): Promise<boolean> {
  try {
    const { code } = await new Deno.Command(bin, {
      args,
      stdout: "null",
      stderr: "null",
    }).output();
    return code === 0;
  } catch (_e) {
    return false;
  }
}

// Verify the host has clang and lld on Linux.
//   - clang: .cargo/config.toml pins linker=clang for the Linux gnu targets.
//   - lld: release builds set lto=true, producing LLVM-bitcode rlibs that GNU ld
//     cannot read, so getRustflags adds -fuse-ld=lld on Linux.
export async function assertHostToolchain(): Promise<void> {
  if (toolchainChecked) return;
  if (Deno.build.os !== "linux") {
    toolchainChecked = true;
    return;
  }

  const missing: string[] = [];
  if (!(await runs("clang", ["--version"]))) missing.push("clang");
  if (!(await runs("ld.lld", ["--version"]))) missing.push("ld.lld");

  if (missing.length > 0) {
    console.error(
      red(`Error: ${missing.join(" and ")} not found on PATH.`) + "\n\n" +
        "Linux builds link through clang with lld. Install both:\n" +
        "  Debian/Ubuntu:  sudo apt install clang lld\n" +
        "  Fedora:         sudo dnf install clang lld\n" +
        "  Arch:           sudo pacman -S clang lld\n",
    );
    Deno.exit(1);
  }

  toolchainChecked = true;
}

// Build the RUSTFLAGS string for a (host, target) combination. Composed in one
// place so we can layer host-specific concerns (e.g. -fuse-ld=lld on Linux) on
// top of target-specific concerns (e.g. ios undefined-symbols workaround).
export function getRustflags(target?: string): string {
  const parts: string[] = [];

  if (target == null) {
    parts.push("-C target-cpu=native");
  } else if (target.includes("ios")) {
    parts.push("-C link-arg=-Wl,-U,___chkstk_darwin");
  }

  // On Linux, the release profile's LTO emits bitcode rlibs that only lld can
  // link. .cargo/config.toml pins linker=clang for Linux targets; this flag
  // tells clang to invoke lld as the underlying linker.
  if (Deno.build.os === "linux") {
    parts.push("-C link-arg=-fuse-ld=lld");
  }

  // Windows MSVC links a fully static CRT (see .cargo/config.toml's
  // `+crt-static` and the /MT CFLAGS used for the XNNPACK C build). Setting
  // RUSTFLAGS as an env var *overrides* the `[target.*].rustflags` from
  // .cargo/config.toml, so re-assert crt-static here or it is silently dropped
  // and the CRT model diverges from the C dependencies (dllimport link errors).
  if ((target ?? getHostTriple()).includes("windows")) {
    parts.push("-C target-feature=+crt-static");
  }

  return parts.join(" ");
}

// Build tool to use for compilation
export enum BuildTool {
  Cargo = "cargo",
  Cross = "cross",
  CargoXwin = "cargo-xwin",
  CargoNdk = "cargo-ndk",
}

// Convert BuildTool to command array
export function buildToolToCommand(tool: BuildTool): string[] {
  switch (tool) {
    case BuildTool.Cargo:
      return ["cargo"];
    case BuildTool.Cross:
      return ["cross"];
    case BuildTool.CargoXwin:
      return ["cargo", "xwin"];
    case BuildTool.CargoNdk:
      return ["cargo", "ndk"];
  }
}

// Get the host platform triple
export function getHostTriple(): string {
  const os = Deno.build.os;
  const arch = Deno.build.arch;

  if (os === "linux") {
    return `${arch}-unknown-linux-gnu`;
  } else if (os === "darwin") {
    return `${arch}-apple-darwin`;
  } else if (os === "windows") {
    return `${arch}-pc-windows-msvc`;
  }

  throw new Error(`Unsupported platform: ${os}-${arch}`);
}

// Environment variables for every cargo invocation.
export function getEnvVars(target?: string): Record<string, string> {
  const actualTarget = target || getHostTriple();

  const env: Record<string, string> = {
    // Build box-format's liblzma from source and link it statically, rather
    // than linking whatever liblzma the host has.
    LZMA_API_STATIC: "1",
  };

  if (actualTarget.includes("windows")) {
    env.CFLAGS = "/MT"; // Static CRT for C deps (cc crate)
    env.CXXFLAGS = "/EHsc /MT"; // Enable C++ exceptions + static CRT

    // [profile.release] asks for fat LTO, codegen-units = 1 and full debug
    // info. Across executorch, onig, zstd, lzma and blake3 that puts the whole
    // program in one LLVM module and runs the CI agents out of memory:
    //
    //     rustc-LLVM ERROR: out of memory
    //     Allocation failed
    //
    // Thin LTO keeps most of the optimisation at a fraction of the peak, and
    // release builds do not need full debug info. Applied through CARGO_PROFILE_*
    // rather than Cargo.toml so only Windows changes and the macOS and Linux
    // release artifacts keep the original settings. Incremental is off because
    // it does nothing under LTO but cost disk and memory.
    env.CARGO_PROFILE_RELEASE_LTO = "thin";
    env.CARGO_PROFILE_RELEASE_DEBUG = "1";
    env.CARGO_PROFILE_RELEASE_INCREMENTAL = "false";
  }

  return env;
}

// Execute a command with environment variables
export async function exec(
  cmd: string[],
  env: Record<string, string> = {},
): Promise<void> {
  const command = new Deno.Command(cmd[0], {
    args: cmd.slice(1),
    env: { ...Deno.env.toObject(), ...env },
    stdout: "inherit",
    stderr: "inherit",
  });

  const { code } = await command.output();
  if (code !== 0) {
    Deno.exit(code);
  }
}

// Strip binary symbols
export async function stripBinary(
  target?: string,
  debug = false,
): Promise<void> {
  const actualTarget = target || getHostTriple();
  if (actualTarget.includes("windows")) {
    // Windows binaries don't use strip; debug info is in .pdb files
    return;
  }

  const buildType = debug ? "debug" : "release";
  const targetPath = target ? `${target}/` : "";
  const binaryPath = `./target/${targetPath}${buildType}/divvun-runtime`;

  console.log(cyan("Stripping binary: ") + dim(binaryPath));
  await exec(["strip", "-x", "-S", binaryPath]);
}

type OsFamily = "android" | "windows" | "apple" | "linux";

// OS family of a target triple. Android is checked first because its triples
// (aarch64-linux-android) also match linux.
function osFamily(triple: string): OsFamily | null {
  if (triple.includes("android")) return "android";
  if (triple.includes("windows")) return "windows";
  if (triple.includes("apple")) return "apple";
  if (triple.includes("linux")) return "linux";
  return null;
}

// Determine which build tool to use for the target
export function needsCrossCompile(host: string, target?: string): BuildTool {
  if (!target || host === target) {
    return BuildTool.Cargo;
  }

  const hostOs = osFamily(host);
  const targetOs = osFamily(target);

  // Android targets use cargo-ndk
  if (targetOs === "android") {
    return BuildTool.CargoNdk;
  }

  // Same OS, different arch: the native toolchain cross-compiles directly.
  // Covers apple x86_64 ↔ aarch64, linux gnu ↔ musl, and windows x86_64 →
  // aarch64 via the ARM64 MSVC tools.
  if (hostOs !== null && hostOs === targetOs) {
    return BuildTool.Cargo;
  }

  // Windows targets from a non-Windows host have no MSVC, so use the CRT and
  // SDK that xwin provides.
  if (targetOs === "windows") {
    return BuildTool.CargoXwin;
  }

  // Linux targets from a non-Linux host go through cross's Docker images.
  if (targetOs === "linux") {
    return BuildTool.Cross;
  }

  // Anything left (e.g. an Apple target from a non-Apple host) has no working
  // toolchain. Fail loudly instead of dispatching to a tool that cannot service
  // the target and failing later with a confusing error.
  throw new Error(`No build tool can target ${target} from ${host}`);
}
