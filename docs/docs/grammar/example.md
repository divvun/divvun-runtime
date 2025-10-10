# Complete Example

Complete grammar checker project from scratch.

## Project Structure

```
my-grammar-checker/
├── pipeline.ts
├── assets/
│   ├── tokeniser-gramcheck-gt-desc.pmhfst
│   ├── analyser-gt-whitespace.hfst
│   ├── valency.bin
│   ├── mwe-dis.bin
│   ├── errmodel.default.hfst
│   ├── acceptor.default.hfst
│   ├── valency-postspell.bin
│   ├── grammarchecker.bin
│   ├── generator-gramcheck-gt-norm.hfstol
│   ├── errors.json
│   ├── errors-en.ftl
│   └── errors-fo.ftl
└── .divvun-rt/          # Generated
```

## Step 1: Initialize Project

```bash
mkdir my-grammar-checker
cd my-grammar-checker
divvun-runtime init
```

## Step 2: Create Pipeline

**pipeline.ts**:

```typescript
import { Command, StringEntry } from './.divvun-rt/mod.ts';
import * as hfst from './.divvun-rt/hfst.ts';
import * as cg3 from './.divvun-rt/cg3.ts';
import * as divvun from './.divvun-rt/divvun.ts';

/**
 * Complete grammar checker with spell checking.
 */
export default function grammarChecker(entry: StringEntry): Command {
    // Tokenize
    let x = hfst.tokenize(entry, {
        model_path: "tokeniser-gramcheck-gt-desc.pmhfst"
    });

    // Whitespace analysis
    x = divvun.blanktag(x, {
        model_path: "analyser-gt-whitespace.hfst"
    });

    // Disambiguation
    x = cg3.vislcg3(x, { model_path: "valency.bin" });

    // MWE handling
    x = cg3.mwesplit(x);
    x = cg3.vislcg3(x, { model_path: "mwe-dis.bin" });

    // Spell checking
    x = divvun.cgspell(x, {
        err_model_path: "errmodel.default.hfst",
        acc_model_path: "acceptor.default.hfst",
        config: {
            n_best: 10,
            max_weight: 5000.0,
            beam: 15.0,
            recase: true
        }
    });

    // Post-spell processing
    x = cg3.vislcg3(x, { model_path: "valency-postspell.bin" });

    // Grammar checking
    x = cg3.vislcg3(x, { model_path: "grammarchecker.bin" });

    // Generate error report
    return divvun.suggest(x, {
        model_path: "generator-gramcheck-gt-norm.hfstol"
    });
}

/**
 * Spell-only checker (faster).
 */
export function spellOnly(entry: StringEntry): Command {
    let x = hfst.tokenize(entry, {
        model_path: "tokeniser-gramcheck-gt-desc.pmhfst"
    });

    x = divvun.cgspell(x, {
        err_model_path: "errmodel.default.hfst",
        acc_model_path: "acceptor.default.hfst"
    });

    return divvun.suggest(x, {
        model_path: "generator-gramcheck-gt-norm.hfstol"
    });
}

/**
 * Dev pipeline for testing with local models.
 */
export function localTest_dev(entry: StringEntry): Command {
    let x = hfst.tokenize(entry, {
        model_path: "@../test-models/tokeniser.pmhfst"
    });

    return divvun.suggest(x, {
        model_path: "@../test-models/generator.hfstol"
    });
}
```

## Step 3: Create errors.json

**assets/errors.json**:

```json
{
  "real-word-error": [
    { "re": "^lex-.*" }
  ],
  "typo": [
    { "id": "spelling-error" }
  ],
  "msyn-verb-form": [
    { "id": "verb-form-error" }
  ],
  "agr-subj-verb": [
    { "id": "subject-verb-agreement" }
  ],
  "agr-noun-adj": [
    { "id": "adjective-agreement" }
  ],
  "wrong-case": [
    { "id": "case-error" }
  ]
}
```

## Step 4: Create Fluent Files

**assets/errors-en.ftl**:

```fluent
# Spelling errors
spelling-error = Spelling error
    .desc = The word {$1} is not in the dictionary.

# Lexical errors
real-word-error = Wrong word
    .desc = The word {$1} exists but seems wrong here. Try {€1}.

# Verb errors
verb-form-error = Wrong verb form
    .desc = Use {€1} instead of {$1} after this auxiliary.

# Agreement errors
subject-verb-agreement = Subject-verb agreement
    .desc = The verb {$1} should agree with the subject in number.

adjective-agreement = Adjective agreement
    .desc = The adjective {$1} should agree with {$2}.

# Case errors
case-error = Wrong case
    .desc = Use the {€1} case here, not {$1}.
```

**assets/errors-fo.ftl**:

```fluent
# Stavavilluir
spelling-error = Stavavillufeilur
    .desc = Orðið {$1} er ikki í orðabókini.

# Orðval
real-word-error = Skeivt orð
    .desc = Orðið {$1} finst men sæst skeivt her. Royn {€1}.

# Verbfeilir
verb-form-error = Skeiv verbform
    .desc = Nýt {€1} ístaðin fyri {$1} aftaná hesum hjálparverbi.

# Samsvarsfeilir
subject-verb-agreement = Frumlag-sagnsamsvør
    .desc = Verbið {$1} skal samsvara við frumlagið í tali.

adjective-agreement = Lýsingarorðssamsvør
    .desc = Lýsingarorðið {$1} skal samsvara við {$2}.

# Kassusfeilir
case-error = Skeiv kasus
    .desc = Nýt {€1} hér, ikki {$1}.
```

## Step 5: Add Model Files

Place your HFST and CG3 models in `assets/`:

```bash
cp /path/to/models/*.pmhfst assets/
cp /path/to/models/*.hfst assets/
cp /path/to/models/*.bin assets/
cp /path/to/models/*.hfstol assets/
```

## Step 6: Test Pipeline

```bash
# Run from TypeScript
divvun-runtime run ./pipeline.ts "I has went to school"

# Run spell-only
divvun-runtime run --pipeline spell-only ./pipeline.ts "wrold"

# Run dev pipeline (not included in bundle)
divvun-runtime run --pipeline local-test ./pipeline.ts "test"
```

## Step 7: Create Bundle

```bash
divvun-runtime bundle
```

Output:
```
Skipping 1 dev pipeline(s): local-test
Type checking pipeline.ts
Validating assets
Creating bundle.drb
Done
```

## Step 8: Test Bundle

```bash
# Default pipeline
divvun-runtime run bundle.drb "I has went to school"

# With configuration
divvun-runtime run \
  -c 'suggest={"locales":["fo","en"],"ignore":["typo"]}' \
  bundle.drb \
  "Text to check"

# List available pipelines
divvun-runtime list bundle.drb
```

Output:
```
Bundle bundle.drb
Pipelines 2 available

• grammar-checker (default)
• spell-only
```

## Expected Output

Input: "I has went to school"

Output:
```json
[
  {
    "form": "has went",
    "beg": 2,
    "end": 10,
    "err": "msyn-verb-form",
    "msg": [
      "Wrong verb form",
      "Use gone instead of went after this auxiliary."
    ],
    "rep": ["has gone"]
  }
]
```

## Development Workflow

1. **Edit grammar rules**: Modify CG3 .bin files
2. **Update errors**: Add new error tags
3. **Map errors**: Update errors.json
4. **Add messages**: Update .ftl files
5. **Test**: Run pipeline with test cases
6. **Bundle**: Create production bundle
7. **Distribute**: Share bundle.drb

## Testing Strategy

Create test file:

**test-cases.txt**:
```
I has went to school
The big houses is red
She dont like pizza
```

Run tests:
```bash
while IFS= read -r line; do
    echo "Testing: $line"
    divvun-runtime run bundle.drb "$line"
done < test-cases.txt
```

## Next Steps

- Learn about [Advanced Topics](../reference/advanced.md)
- Explore [Module Reference](../modules.md)
- See [Configuration options](../reference/configuration.md)
