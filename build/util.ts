import { cyan, dim, red, yellow } from "jsr:@std/fmt@1/colors";
import * as path from "jsr:@std/path@1";

const MIN_LLVM_MAJOR = 19;
let toolchainChecked = false;

async function probeVersion(
  bin: string,
  args: string[],
  pattern: RegExp,
): Promise<{ major: number; raw: string } | null> {
  let stdoutText: string;
  try {
    const { code, stdout } = await new Deno.Command(bin, {
      args,
      stdout: "piped",
      stderr: "null",
    }).output();
    if (code !== 0) return null;
    stdoutText = new TextDecoder().decode(stdout);
  } catch (_e) {
    return null;
  }
  const m = pattern.exec(stdoutText);
  if (!m) return { major: NaN, raw: stdoutText };
  return { major: Number(m[1]), raw: stdoutText };
}

// Verify the host LLVM toolchain (clang + lld) is new enough on Linux.
//   - clang: cg3-rs and hfst-rs hardcode `clang++` as the C++ compiler. Clang ≤ 16
//     cannot parse libstdc++-15's C++23/26 headers (std::format_kind, auto(x) in
//     <ranges>, new pair/tuple constraints), which is the default on Debian Sid
//     / Fedora 41+.
//   - lld: release builds set lto=true, producing LLVM-bitcode rlibs that GNU ld
//     cannot read ("file format not recognized"). .cargo/config.toml pins
//     linker=clang on Linux, and we add -fuse-ld=lld via RUSTFLAGS, so lld must
//     be installed.
export async function assertHostToolchain(): Promise<void> {
  if (toolchainChecked) return;
  if (Deno.build.os !== "linux") {
    toolchainChecked = true;
    return;
  }

  const installHint =
    "Install clang and lld (need ≥ " + MIN_LLVM_MAJOR + ").\n" +
    "  Debian/Ubuntu:  sudo apt install clang lld\n" +
    "  Fedora:         sudo dnf install clang lld\n" +
    "  Arch:           sudo pacman -S clang lld\n";

  const clang = await probeVersion("clang", ["--version"], /clang version (\d+)/);
  if (!clang) {
    console.error(
      red("Error: clang not found on PATH (or `clang --version` failed).") +
        "\n" +
        "cg3-rs and hfst-rs both invoke `clang++` directly; please install clang ≥ " +
        MIN_LLVM_MAJOR + ".\n\n" + installHint,
    );
    Deno.exit(1);
  }
  if (Number.isNaN(clang.major)) {
    console.error(
      yellow("Warning: could not parse clang version from:") + "\n" + clang.raw,
    );
  } else if (clang.major < MIN_LLVM_MAJOR) {
    console.error(
      red(`Error: clang ${clang.major} is too old (need ≥ ${MIN_LLVM_MAJOR}).`) +
        "\n\n" +
        "cg3-rs and hfst-rs hardcode `clang++` as the C++ compiler. Clang ≤ 16\n" +
        "cannot parse libstdc++-15's C++23/26 headers, which is the default\n" +
        "system libstdc++ on Debian Sid, Fedora 41+, and other rolling distros.\n\n" +
        "Fix: install a newer clang and ensure plain `clang` / `clang++` on\n" +
        "PATH resolve to it. Setting CC/CXX does NOT help — cg3-rs hardcodes\n" +
        "the binary names.\n\n" + installHint,
    );
    Deno.exit(1);
  }

  const lld = await probeVersion("ld.lld", ["--version"], /LLD (\d+)/);
  if (!lld) {
    console.error(
      red("Error: ld.lld not found on PATH (or `ld.lld --version` failed).") +
        "\n\n" +
        "Release builds use LTO, which produces LLVM-bitcode rlibs that GNU ld\n" +
        "cannot read. We pin clang+lld as the Linux linker; lld must be on PATH.\n\n" +
        installHint,
    );
    Deno.exit(1);
  }
  if (Number.isNaN(lld.major)) {
    console.error(
      yellow("Warning: could not parse lld version from:") + "\n" + lld.raw,
    );
  } else if (lld.major < MIN_LLVM_MAJOR) {
    console.error(
      red(`Error: ld.lld ${lld.major} is too old (need ≥ ${MIN_LLVM_MAJOR}).`) +
        "\n\n" + installHint,
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

// Get environment variables with sysroot path
export function getEnvVars(target?: string): Record<string, string> {
  const actualTarget = target || getHostTriple();
  const sysroot = Deno.realPathSync(
    path.join(import.meta.dirname ?? "", "..", ".x", "sysroot", actualTarget),
  );
  const srcroot = path.join(import.meta.dirname ?? "", "..", ".x", "packages", "src");

  console.log(dim(`Using sysroot at: ${sysroot}`));

  const env: Record<string, string> = {
    LZMA_API_STATIC: "1",
    HFST_SYSROOT: sysroot,
    CG3_SYSROOT: sysroot,
    EXECUTORCH_SYSROOT: sysroot,
  };

  // Use clang-cl on Windows for C/C++ compilation
  if (actualTarget.includes("windows")) {
    // env.CC = "clang-cl";
    // env.CXX = "clang-cl";
    // env.LD = "lld-link";
    // env.AR = "llvm-lib";
    env.CFLAGS = "/MT"; // Static CRT for C deps (cc crate)
    env.CXXFLAGS = "/EHsc /MT"; // Enable C++ exceptions + static CRT

    // Add MSYS2 to PATH so cmake can find flex, bison
    const msys2Bin = "C:\\msys64\\usr\\bin";
    const currentPath = Deno.env.get("PATH") || "";
    env.PATH = `${currentPath};${msys2Bin}`;
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

// Determine which build tool to use for the target
export function needsCrossCompile(host: string, target?: string): BuildTool {
  if (!target) {
    return BuildTool.Cargo;
  }

  // Android targets use cargo-ndk
  if (target.includes("android")) {
    return BuildTool.CargoNdk;
  }

  // Windows targets from Unix hosts use cargo-xwin
  if (!host.includes("windows") && target.includes("windows")) {
    return BuildTool.CargoXwin;
  }

  // Apple-to-apple cross-compilation (x86_64 ↔ aarch64) uses cargo
  if (host.includes("apple") && target.includes("apple")) {
    return BuildTool.Cargo;
  }

  // Linux-to-linux cross-compilation uses cargo (native compilers available in CI)
  if (host.includes("linux") && target.includes("linux")) {
    return BuildTool.Cargo;
  }

  // Different architectures use cross
  if (host !== target) {
    return BuildTool.Cross;
  }

  return BuildTool.Cargo;
}

// Get sysroot path for target
export function getSysrootPath(target: string): string {
  return `.x/sysroot/${target}`;
}

// Get environment variables for cross-compilation with sysroot
export function getSysrootEnv(target: string): Record<string, string> {
  const sysrootPath = getSysrootPath(target);

  return {
    SYSROOT: sysrootPath,
    // Point pkg-config at the sysroot
    PKG_CONFIG_PATH: `${sysrootPath}/lib/pkgconfig`,
    PKG_CONFIG_SYSROOT_DIR: sysrootPath,
    // Library search paths
    LIBRARY_PATH: `${sysrootPath}/lib`,
    LD_LIBRARY_PATH: `${sysrootPath}/lib`,
    // Include paths
    C_INCLUDE_PATH: `${sysrootPath}/include`,
    CPLUS_INCLUDE_PATH: `${sysrootPath}/include`,
  };
}
