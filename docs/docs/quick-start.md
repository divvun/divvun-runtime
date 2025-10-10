# Quick Start

We're going to build a simple text processing pipeline. By the end, you'll understand how pipelines work and be able to create your own.

## What We're Building

A pipeline that takes text, reverses it, and converts to uppercase. Simple, but it shows how commands chain together.

## Create the Project

```bash
mkdir my-pipeline
cd my-pipeline
divvun-runtime init
```

You now have `pipeline.ts` and a `.divvun-rt/` directory with TypeScript types.

## What Just Happened

Open `pipeline.ts` and you'll see:

```typescript
import { Command, StringEntry } from './.divvun-rt/mod.ts';
import * as example from './.divvun-rt/example.ts';

export default function examplePipeline(entry: StringEntry): Command {
    let x = example.reverse(entry);
    x = example.upper(x);
    return x;
}
```

Breaking this down:
- `StringEntry` is your input type (a string)
- `entry` is the actual input text
- `example.reverse(entry)` reverses the string
- `example.upper(x)` uppercases it
- The function returns the result

The `example` module is just for learning. Real pipelines use `hfst`, `cg3`, `divvun`, and `speech`.

## Run It

```bash
divvun-runtime run ./pipeline.ts "Hello World"
```

Output:
```
DLROW OLLEH
```

The text flows through: input → reverse → uppercase → output.

## Modify It

Change the pipeline to just uppercase:

```typescript
export default function myPipeline(entry: StringEntry): Command {
    return example.upper(entry);
}
```

Run it again - now it just uppercases without reversing.

## Use Real Modules

The `example` module is for learning. Real pipelines look like this:

```typescript
import { StringEntry, Command } from './.divvun-rt/mod.ts';
import * as hfst from './.divvun-rt/hfst.ts';
import * as cg3 from './.divvun-rt/cg3.ts';
import * as divvun from './.divvun-rt/divvun.ts';

export default function grammarCheck(entry: StringEntry): Command {
    let x = hfst.tokenize(entry, { model_path: "tokeniser.pmhfst" });
    x = cg3.vislcg3(x, { model_path: "grammar.bin" });
    return divvun.suggest(x, { model_path: "generator.hfstol" });
}
```

This tokenizes text, applies grammar rules, and generates error suggestions. You'd need to add the model files to `assets/` for this to work.

## What You Learned

- Pipelines are TypeScript functions
- Commands chain together: one's output becomes the next's input
- `divvun-runtime run` executes your pipeline
- `.divvun-rt/` contains generated types for all available commands

See [Pipelines](./pipelines.md) for more details, or jump to [Grammar Checking](./grammar/overview.md) or [Text-to-Speech](./tts/overview.md) to build something real.
