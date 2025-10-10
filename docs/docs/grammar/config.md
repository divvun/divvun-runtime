# Configuration Objects

Grammar checking modules accept configuration objects for tuning behavior.

## divvun::cgspell Configuration

### SpellerConfig

```typescript
interface SpellerConfig {
  n_best?: number;           // Max suggestions per word (default: 10)
  max_weight?: number;        // Max suggestion weight (default: 5000.0)
  beam?: number;              // Weight range (default: 15.0)
  reweight?: {                // Edit distance penalties
    start_penalty: number;
    end_penalty: number;
    mid_penalty: number;
  };
  node_pool_size?: number;    // Parallel processing (default: auto)
  continuation_marker?: string; // Unfinished word marker
  recase?: boolean;           // Try recasing (default: false)
}
```

### Usage

```typescript
x = divvun.cgspell(x, {
    err_model_path: "errmodel.default.hfst",
    acc_model_path: "acceptor.default.hfst",
    config: {
        n_best: 10,
        max_weight: 5000.0,
        beam: 15.0,
        reweight: {
            start_penalty: 10.0,
            end_penalty: 10.0,
            mid_penalty: 5.0
        },
        recase: true
    }
});
```

### Parameters Explained

**n_best**: Maximum number of suggestions
- Higher = more suggestions but slower
- Lower = faster but fewer options
- Typical: 5-15

**max_weight**: Maximum edit distance weight
- Higher = more distant suggestions allowed
- Lower = only close matches
- Typical: 3000-7000

**beam**: Weight difference between best and worst suggestion
- Controls quality spread
- Typical: 10-20

**reweight**: Edit distance penalties
- `start_penalty`: Errors at word start
- `end_penalty`: Errors at word end
- `mid_penalty`: Errors in middle
- Higher penalty = less likely suggestion

**recase**: Try recasing before other suggestions
- Suggests case-only changes first
- Useful for proper nouns

### Examples

**Fast, few suggestions**:
```typescript
config: {
    n_best: 5,
    max_weight: 3000.0,
    beam: 10.0
}
```

**Thorough, many suggestions**:
```typescript
config: {
    n_best: 20,
    max_weight: 7000.0,
    beam: 20.0
}
```

**Prioritize word endings**:
```typescript
config: {
    reweight: {
        start_penalty: 5.0,
        end_penalty: 15.0,
        mid_penalty: 8.0
    }
}
```

## cg3::vislcg3 Configuration

### Vislcg3Config

```typescript
interface Vislcg3Config {
  trace?: boolean;  // Enable trace output (default: false)
}
```

### Usage

```typescript
x = cg3.vislcg3(x, {
    model_path: "grammar.bin",
    config: {
        trace: true
    }
});
```

### trace Parameter

Enables detailed rule execution logging:

```
EXECUTE: @123 SELECT N IF (1 V)
  MATCHED at cohort 5
  REMOVED reading: "word" Adj
```

Useful for:
- Debugging grammar rules
- Understanding disambiguation
- Developing new rules

**Warning**: Generates large output. Only use during development.

## divvun::suggest Runtime Configuration

Configuration passed via `-c` flag or per-execution config:

```typescript
interface SuggestRuntimeConfig {
  locales?: string[];     // Message locale priority
  encoding?: string;      // Position encoding: "bytes" | "utf-16"
  ignore?: string[];      // Error types to suppress
}
```

### Usage

```bash
# Set locales
divvun-runtime run -c 'suggest={"locales":["fo","en"]}' bundle.drb "text"

# Set encoding
divvun-runtime run -c 'suggest={"encoding":"utf-16"}' bundle.drb "text"

# Ignore errors
divvun-runtime run -c 'suggest={"ignore":["typo","lex-*"]}' bundle.drb "text"
```

### locales

Priority list for error messages:

```bash
-c 'suggest={"locales":["fo","en"]}'
```

1. Try Faroese (`errors-fo.ftl`)
2. Fall back to English (`errors-en.ftl`)
3. Use error tag if no message found

### encoding

Position encoding for error locations:

- `"bytes"` (default): Byte offsets
- `"utf-16"`: UTF-16 code units

Use UTF-16 for JavaScript/TypeScript clients:
```typescript
const text = "Hello world";
const errPos = 6; // UTF-16 position
```

### ignore

Suppress specific error types:

```bash
# Ignore spelling errors
-c 'suggest={"ignore":["typo"]}'

# Ignore lexical errors
-c 'suggest={"ignore":["lex-*"]}'

# Ignore multiple types
-c 'suggest={"ignore":["typo","msyn-*","lex-ta-tad"]}'
```

Patterns support wildcards (*).

## Complete Example

```typescript
import { Command, StringEntry } from './.divvun-rt/mod.ts';
import * as hfst from './.divvun-rt/hfst.ts';
import * as cg3 from './.divvun-rt/cg3.ts';
import * as divvun from './.divvun-rt/divvun.ts';

export default function grammarChecker(entry: StringEntry): Command {
    let x = hfst.tokenize(entry, {
        model_path: "tokeniser-gramcheck-gt-desc.pmhfst"
    });

    x = divvun.blanktag(x, {
        model_path: "analyser-gt-whitespace.hfst"
    });

    // Use trace for debugging
    x = cg3.vislcg3(x, {
        model_path: "valency.bin",
        config: { trace: false }
    });

    x = cg3.mwesplit(x);
    x = cg3.vislcg3(x, { model_path: "mwe-dis.bin" });

    // Configure spell checker
    x = divvun.cgspell(x, {
        err_model_path: "errmodel.default.hfst",
        acc_model_path: "acceptor.default.hfst",
        config: {
            n_best: 10,
            max_weight: 5000.0,
            beam: 15.0,
            reweight: {
                start_penalty: 10.0,
                end_penalty: 10.0,
                mid_penalty: 5.0
            },
            recase: true
        }
    });

    x = cg3.vislcg3(x, { model_path: "valency-postspell.bin" });
    x = cg3.vislcg3(x, { model_path: "grammarchecker.bin" });

    // Runtime config via -c flag:
    // -c 'suggest={"locales":["fo","en"],"ignore":["typo"]}'
    return divvun.suggest(x, {
        model_path: "generator-gramcheck-gt-norm.hfstol"
    });
}
```

Run with configuration:
```bash
divvun-runtime run \
  -c 'suggest={"locales":["fo","en"],"encoding":"utf-16","ignore":["typo"]}' \
  bundle.drb \
  "Text to check"
```

## Performance Tuning

### Fast Mode (fewer suggestions)
```typescript
config: {
    n_best: 5,
    max_weight: 3000.0,
    beam: 10.0
}
```

### Quality Mode (more suggestions)
```typescript
config: {
    n_best: 15,
    max_weight: 6000.0,
    beam: 18.0
}
```

### Balanced (default)
```typescript
config: {
    n_best: 10,
    max_weight: 5000.0,
    beam: 15.0
}
```

## Next Steps

- See [Complete Example](./example.md) for full setup
- Learn about [Advanced Topics](../reference/advanced.md)
- Explore [Module Reference](../modules.md) for all options
