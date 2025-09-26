import * as path from "jsr:@std/path";

let libPath: string | null = null;

export function setLibPath(newPath: string) {
  libPath = newPath;
}

let dylib: Deno.DynamicLibrary<Record<string, Deno.ForeignFunction>>;

const RustSliceT = { "struct": ["pointer", "usize"] } as const;

function loadDylib() {
  let libSuffix = "";
  
  switch (Deno.build.os) {
  case "windows":
    libSuffix = "dll";
    break;
  case "darwin":
    libSuffix = "dylib";
    break;
  default:
    libSuffix = "so";
    break;
  }

  const libName = `libdivvun_runtime.${libSuffix}`;
  const fullLibPath = libPath ? path.join(libPath, libName) : libName;

  dylib = Deno.dlopen(fullLibPath, {
    DRT_Bundle_fromBundle: { parameters: [RustSliceT, "function"], result: "pointer" },
    DRT_Bundle_drop: { parameters: ["pointer"], result: "void" },
    DRT_Bundle_fromPath: { parameters: [RustSliceT, "function"], result: "pointer" },
    DRT_Bundle_create: { parameters: ["pointer", RustSliceT, "function"], result: "pointer" },
    DRT_PipelineHandle_drop: { parameters: ["pointer"], result: "void" },
    DRT_PipelineHandle_forward: { parameters: ["pointer", RustSliceT, "function"], result: RustSliceT },
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
      
      const message = new TextDecoder().decode(new Uint8Array(Deno.UnsafePointerView.getArrayBuffer(ptr, Number(len))));
      throw new Error(message);
  },
);

function makeRustString(str: string): ArrayBuffer {
  const encoded = encoder.encode(str);
  const ptr = Deno.UnsafePointer.of<Uint8Array>(encoded);
  
  return new BigUint64Array([
    Deno.UnsafePointer.value(ptr),
    BigInt(encoded.length),
  ]).buffer
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

      return new Bundle(bundleRawPtr)
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

      return new Bundle(bundleRawPtr)
    } catch (e) {
      throw e;
    }
  }

  private constructor(ptr: Deno.PointerValue) {
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
      const pipeRawPtr: Deno.PointerValue<PipelineHandle> = dylib.symbols.DRT_Bundle_create(
        this.#ptr,
        rsConfig,
        errCallback.pointer,
      ) as Deno.PointerValue<PipelineHandle>;

      return new PipelineHandle(pipeRawPtr)
    } catch (e) {
      throw e;
    }
  }
}

class PipelineResponse {
  #buf: Uint8Array | null;
  #ptr: Deno.PointerValue;
  #len: number;

  constructor(buf: Uint8Array) {
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

  constructor(ptr: Deno.PointerValue) {
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
      return new PipelineResponse(outputSlice);
    } catch (e) {
      throw e;
    }
  }
}