# Grammar Checking Overview

Grammar checking in Divvun Runtime combines morphological analysis, disambiguation, spell checking, and grammar rule application to detect and correct errors in text.

## What is Grammar Checking?

Grammar checking is a multi-stage pipeline that:

1. **Tokenizes** text into analyzable units
2. **Analyzes** word forms morphologically
3. **Disambiguates** readings using context
4. **Checks** spelling and generates corrections
5. **Applies** grammar rules to detect errors
6. **Generates** suggestions for corrections

## Output Format

Grammar checkers return JSON with error information:

```json
[
  {
    "form": "wrod",
    "beg": 0,
    "end": 4,
    "err": "typo",
    "msg": ["Spelling error", "Word not in dictionary"],
    "rep": ["word", "world", "wor"]
  },
  {
    "form": "has went",
    "beg": 10,
    "end": 18,
    "err": "msyn-verb-form",
    "msg": ["Wrong verb form", "Use 'gone' after 'has'"],
    "rep": ["has gone"]
  }
]
```

## Pipeline Architecture

A typical grammar checker pipeline includes:

```typescript
export default function grammarChecker(entry: StringEntry): Command {
    // Stage 1: Tokenization
    let x = hfst.tokenize(entry, {
        model_path: "tokeniser-gramcheck-gt-desc.pmhfst"
    });

    // Stage 2: Whitespace analysis
    x = divvun.blanktag(x, {
        model_path: "analyser-gt-whitespace.hfst"
    });

    // Stage 3: Disambiguation
    x = cg3.vislcg3(x, { model_path: "valency.bin" });

    // Stage 4: MWE handling
    x = cg3.mwesplit(x);
    x = cg3.vislcg3(x, { model_path: "mwe-dis.bin" });

    // Stage 5: Spell checking
    x = divvun.cgspell(x, {
        err_model_path: "errmodel.default.hfst",
        acc_model_path: "acceptor.default.hfst"
    });

    // Stage 6: Post-spell disambiguation
    x = cg3.vislcg3(x, { model_path: "valency-postspell.bin" });

    // Stage 7: Grammar checking
    x = cg3.vislcg3(x, { model_path: "grammarchecker.bin" });

    // Stage 8: Generate suggestions
    return divvun.suggest(x, {
        model_path: "generator-gramcheck-gt-norm.hfstol"
    });
}
```

## Key Components

### 1. Error Tagging
CG3 rules mark errors with special tags:
```cg3
ADD (&typo) target-pattern ;
ADD (&agreement-error) noun-pattern ;
```

### 2. errors.json Mapping
Maps error tags to Fluent message IDs:
```json
{
  "typo": [{ "id": "spelling-error" }],
  "agreement-error": [{ "re": "^agr-.*" }]
}
```

### 3. Fluent Message Files
Localized error messages:
```fluent
spelling-error = Spelling error
    .desc = The word {$1} is not in the dictionary.

agr-noun-adj = Agreement error
    .desc = The adjective {$1} should agree with the noun {$2}.
```

## Required Assets

A grammar checker project needs:

```
assets/
├── tokeniser-gramcheck-gt-desc.pmhfst
├── analyser-gt-whitespace.hfst
├── valency.bin
├── mwe-dis.bin
├── errmodel.default.hfst
├── acceptor.default.hfst
├── valency-postspell.bin
├── grammarchecker.bin
├── generator-gramcheck-gt-norm.hfstol
├── errors.json
├── errors-en.ftl
└── errors-{locale}.ftl
```

## Workflow Example

**Input text**: "I has went to school"

**Stage 1 - Tokenize**: Break into words
```
"<I>"
"<has>"
"<went>"
"<to>"
"<school>"
```

**Stage 2-4 - Analyze & Disambiguate**: Add linguistic analysis
```
"<has>"
    "have" V Prs Sg3
"<went>"
    "go" V Past
```

**Stage 5 - Check Grammar**: Detect "has went" error
```
"<has>"
    "have" V Prs Sg3 &verb-form-error
```

**Stage 6 - Generate Output**: Create error report
```json
{
  "form": "has went",
  "err": "verb-form-error",
  "msg": ["Wrong verb form", "Use 'gone' after 'has'"],
  "rep": ["has gone"]
}
```

## Runtime Configuration

Configure at runtime with `-c` flag:

```bash
# Set locales for error messages
divvun-runtime run -c 'suggest={"locales":["fo","en"]}' bundle.drb "text"

# Ignore specific error types
divvun-runtime run -c 'suggest={"ignore":["typo"]}' bundle.drb "text"

# Use UTF-16 encoding for positions
divvun-runtime run -c 'suggest={"encoding":"utf-16"}' bundle.drb "text"
```

## Next Steps

- Understand the [Error System](./error-system.md)
- Configure [command options](./config.md)
- See [Complete Example](./example.md)
