import { Bundle, setLibPath } from "../deno/mod.ts";

console.log("Hello");

setLibPath("./target/release");

function main() {
  using bundle = Bundle.fromBundle(
    "/Users/brendan/git/necessary/divvun/pipeline-examples/grammar-sme/bundle.drb",
  );
  using pipe = bundle.create();
  console.log("Pipin'");
  const out = pipe.forward("hi").json();
  console.log(out);
}

main();
