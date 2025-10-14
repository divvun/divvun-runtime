import { Bundle } from "../mod.ts";
export * from "jsr:@std/assert@1";

let bundle: Bundle;

export function loadBundle(bundlePath: string): () => Bundle {
  Deno.test.beforeAll(async () => {
    bundle = await Bundle.fromBundle(bundlePath);
  });

  return () => bundle;
}

export function loadPath(devPath: string): () => Bundle {
  Deno.test.beforeAll(async () => {
    bundle = await Bundle.fromPath(devPath);
  });

  return () => bundle;
}

export function load(): () => Bundle {
  return loadPath(Deno.cwd());
}
