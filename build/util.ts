import { cyan, dim } from "jsr:@std/fmt@1/colors";
import * as path from "jsr:@std/path@1";

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

  console.log(dim(`Using sysroot at: ${sysroot}`));

  const env: Record<string, string> = {
    LZMA_API_STATIC: "1",
    LIBTORCH_BYPASS_VERSION_CHECK: "1",
    LIBTORCH: sysroot,
    HFST_SYSROOT: sysroot,
    CG3_SYSROOT: sysroot,
    LIBTORCH_STATIC: "1",
  };

  // Use clang-cl on Windows for C/C++ compilation
  if (actualTarget.includes("windows")) {
    const llvmPath =
      "C:\\Program Files (x86)\\Microsoft Visual Studio\\2022\\BuildTools\\VC\\Tools\\Llvm\\x64\\bin";
    env.CC = `"${llvmPath}\\clang-cl.exe"`;
    env.CXX = `"${llvmPath}\\clang-cl.exe"`;
    env.LD = `"${llvmPath}\\lld-link.exe"`;
    env.AR = `"${llvmPath}\\llvm-lib.exe"`;
    env.PATH = `${llvmPath};${Deno.env.get("PATH") ?? ""}`;
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
