import { cyan, dim } from "jsr:@std/fmt@1/colors";

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

  return {
    LZMA_API_STATIC: "1",
    LIBTORCH_BYPASS_VERSION_CHECK: "1",
    LIBTORCH: Deno.realPathSync(`.x/sysroot/${actualTarget}`),
    LIBTORCH_STATIC: "1",
  };
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

// Check if cross-compilation tooling is needed
export function needsCross(host: string, target?: string): boolean {
  if (!target) return false;
  return host !== target;
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
