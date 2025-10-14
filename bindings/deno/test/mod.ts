import { Bundle } from "../mod.ts";
export * from "jsr:@std/assert";

let bundle: Bundle;

const sleep = (ms: number) => new Promise((resolve) => setTimeout(() => resolve(null), ms))

export function loadBundle(bundlePath: string) {
  Deno.test.beforeAll(async () => {
    bundle = await Bundle.fromBundle(bundlePath);
  });

  return () => bundle;
}

export function loadPath(devPath: string) {
  Deno.test.beforeAll(async () => {
    bundle = await Bundle.fromPath(devPath);
  });

  return () => bundle;
}

export function load(): () => Bundle {
  return loadPath(Deno.cwd());
}

export async function runGrammar(text: string): Promise<GrammarResponse[]> {
  if (bundle == null) {
    bundle = await Bundle.fromPath(Deno.cwd());
  }

  const pipe = await bundle.create();
  const res = await pipe.forward(text);
  return (await res.json()) as unknown as GrammarResponse[];
}

export type GrammarResponse = {
  form: string;
  beg: number;
  end: number;
  err: string;
  msg: [string, string];
  rep: string[];
};

type DivvunRuntimeModule = typeof import("../mod.ts");

declare global {
  interface Window {
    _DRT?: DivvunRuntimeModule;
  }
  var _DRT: DivvunRuntimeModule | undefined;
}
