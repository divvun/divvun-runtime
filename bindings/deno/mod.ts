const BRAND = Symbol("divvun-runtime")

let libPath: string | null = await findLib();

export function getLibPath(): string | null {
  return libPath;
}

export async function findLib(): Promise<string | null> {
  const name = "divvun-runtime"
  let pathEnv 
  try {
    pathEnv = Deno.env.get("PATH") ?? "";
  } catch (e) {
    pathEnv = "";
  }
  const paths = pathEnv.split(Deno.build.os === "windows" ? ";" : ":");

  const exts = Deno.build.os === "windows"
    ? (Deno.env.get("PATHEXT")?.split(";") ?? [".EXE", ".CMD", ".BAT"])
    : [""];

  for (const dir of paths) {
    for (const ext of exts) {
      const full = `${dir}/${name}${ext}`;
      try {
        const info = await Deno.stat(full);
        if (info.isFile) return full;
      } catch {
        // ignore ENOENT
      }
    }
  }
  return null;
}

export function setLibPath(newPath: string) {
  libPath = newPath;
}

let dylib: Deno.DynamicLibrary<Record<string, Deno.ForeignFunction>>;

const RustSliceT = { "struct": ["pointer", "usize"] } as const;

function loadDylib() {
  if (libPath == null) {
    throw new Error("Could not find divvun-runtime library. Please set the path using setLibPath().");
  }

  dylib = Deno.dlopen(libPath, {
    DRT_Bundle_fromBundle: {
      parameters: [RustSliceT, "function"],
      result: "pointer",
    },
    DRT_Bundle_drop: { parameters: ["pointer"], result: "void" },
    DRT_Bundle_fromPath: {
      parameters: [RustSliceT, "function"],
      result: "pointer",
    },
    DRT_Bundle_create: {
      parameters: ["pointer", RustSliceT, "function"],
      result: "pointer",
    },
    DRT_PipelineHandle_drop: { parameters: ["pointer"], result: "void" },
    DRT_PipelineHandle_forward: {
      parameters: ["pointer", RustSliceT, "function"],
      result: RustSliceT,
    },
    DRT_Vec_drop: { parameters: [RustSliceT], result: "void" },
  });
}

const encoder = new TextEncoder();

const errCallback = new Deno.UnsafeCallback(
  { parameters: ["pointer", "usize"], result: "void" } as const,
  (ptr, len) => {
    if (ptr == null) {
      throw new Error("Unknown error");
    }

    const message = new TextDecoder().decode(
      new Uint8Array(Deno.UnsafePointerView.getArrayBuffer(ptr, Number(len))),
    );
    throw new Error(message);
  },
);

function makeRustString(str: string): ArrayBuffer {
  const encoded = encoder.encode(str);
  const ptr = Deno.UnsafePointer.of<Uint8Array>(encoded);

  return new BigUint64Array([
    Deno.UnsafePointer.value(ptr),
    BigInt(encoded.length),
  ]).buffer;
}

export class Bundle {
  #ptr: Deno.PointerValue;

  public static fromPath(pipelinePath: string): Bundle {
    if (!dylib) {
      loadDylib();
    }

    const rsPipelinePath = makeRustString(pipelinePath);

    try {
      const bundleRawPtr = dylib.symbols.DRT_Bundle_fromPath(
        rsPipelinePath,
        errCallback.pointer,
      ) as Deno.PointerValue<Bundle>;

      return new Bundle(bundleRawPtr, BRAND);
    } catch (e) {
      throw e;
    }
  }

  public static fromBundle(bundlePath: string): Bundle {
    if (!dylib) {
      loadDylib();
    }

    const rsBundlePath = makeRustString(bundlePath);

    try {
      const bundleRawPtr = dylib.symbols.DRT_Bundle_fromBundle(
        rsBundlePath,
        errCallback.pointer,
      ) as Deno.PointerValue<Bundle>;

      return new Bundle(bundleRawPtr, BRAND);
    } catch (e) {
      throw e;
    }
  }

  private constructor(ptr: Deno.PointerValue, brand: symbol) {
    if (brand !== BRAND) {
      throw new TypeError("Bundle must be constructed via fromPath or fromBundle");
    }
    this.#ptr = ptr;
  }

  [Symbol.dispose]() {
    if (this.#ptr) {
      dylib.symbols.DRT_Bundle_drop(this.#ptr);
      this.#ptr = null;
    }
  }

  public create(config: Record<string, unknown> = {}): PipelineHandle {
    if (this.#ptr == null) {
      throw new Error("Bundle has been disposed");
    }

    const configStr = JSON.stringify(config);
    const rsConfig = makeRustString(configStr);

    try {
      const pipeRawPtr: Deno.PointerValue<PipelineHandle> = dylib.symbols
        .DRT_Bundle_create(
          this.#ptr,
          rsConfig,
          errCallback.pointer,
        ) as Deno.PointerValue<PipelineHandle>;

      return new PipelineHandle(pipeRawPtr, BRAND);
    } catch (e) {
      throw e;
    }
  }
}

class PipelineResponse {
  #buf: Uint8Array | null;
  #ptr: Deno.PointerValue;
  #len: number;

  constructor(buf: Uint8Array, brand: symbol) {
    if (brand !== BRAND) {
      throw new TypeError("PipelineResponse cannot be constructed directly");
    }

    this.#buf = buf;

    const ptr = Deno.UnsafePointer.of(buf);

    if (ptr == null) {
      throw new Error("Failed to get output slice pointer");
    }

    const ptrBuf = Deno.UnsafePointerView.getArrayBuffer(ptr, 8);
    const lenBuf = Deno.UnsafePointerView.getArrayBuffer(ptr, 8, 8);

    this.#ptr = Deno.UnsafePointer.create(new BigUint64Array(ptrBuf)[0]);
    this.#len = Number(new BigUint64Array(lenBuf)[0]);
  }

  [Symbol.dispose]() {
    if (this.#buf) {
      dylib.symbols.DRT_Vec_drop(this.#buf);
    }

    this.#buf = null;
    this.#ptr = null;
    this.#len = 0;
  }

  bytes(): Uint8Array {
    if (this.#ptr == null) {
      throw new TypeError("Response has been disposed");
    }

    const dataBuf = Deno.UnsafePointerView.getArrayBuffer(this.#ptr, this.#len);

    try {
      return new Uint8Array(structuredClone(dataBuf));
    } finally {
      this[Symbol.dispose]();
    }
  }

  string(): string {
    if (this.#ptr == null) {
      throw new TypeError("Response has been disposed");
    }

    const dataBuf = Deno.UnsafePointerView.getArrayBuffer(this.#ptr, this.#len);

    try {
      const output = new Uint8Array(dataBuf);
      return new TextDecoder().decode(output);
    } finally {
      this[Symbol.dispose]();
    }
  }

  json(): unknown {
    return JSON.parse(this.string());
  }
}

export class PipelineHandle {
  #ptr: Deno.PointerValue;

  constructor(ptr: Deno.PointerValue, brand: symbol) {
    if (brand !== BRAND) {
      throw new TypeError("PipelineHandle cannot be constructed directly");
    }

    this.#ptr = ptr;
  }

  [Symbol.dispose]() {
    if (this.#ptr) {
      dylib.symbols.DRT_PipelineHandle_drop(this.#ptr);
      this.#ptr = null;
    }
  }

  public forward(input: string): PipelineResponse {
    if (this.#ptr == null) {
      throw new Error("Pipeline has been disposed");
    }

    const rsInput = makeRustString(input);

    try {
      const outputSlice: Uint8Array = dylib.symbols.DRT_PipelineHandle_forward(
        this.#ptr,
        rsInput,
        errCallback.pointer,
      ) as Uint8Array;
      return new PipelineResponse(outputSlice, BRAND);
    } catch (e) {
      throw e;
    }
  }
}
