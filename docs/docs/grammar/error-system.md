# Error System

Grammar checking uses three components: CG3 error tags, errors.json mapping, and Fluent message files.

## errors.json Mapping

Maps error tags to Fluent message IDs. These lists of matches may either be error IDs as emitted by the CG3 output, or a regular expression that matches those.

### Simple Mapping

```json
{
  "typo": [
    { "id": "spelling-error" }
  ],
  "agr-subj-verb": [
    { "id": "subject-verb-agreement" }
  ]
}
```

### Regex Patterns

```json
{
  "real-word-error": [
    { "re": "^lex-.*" },
    { "re": "^msyn-.*" }
  ]
}
```

Matches any error tag starting with `lex-` or `msyn-`.

### Multiple Mappings

First match wins:

```json
{
  "agreement-errors": [
    { "id": "agr-subj-verb" },
    { "id": "agr-noun-adj" },
    { "re": "^agr-.*" }
  ]
}
```

## Fluent Message Files

!!! note
    The format used in the .ftl is unstable and subject to change, as it is presently a rudimentary port of the errors.xml functionality. Things like using € replacements are highly likely to change.

Create `errors-{locale}.ftl` files in `assets/`:

```fluent
spelling-error = Spelling error
    .desc = The word {$1} is not in the dictionary.

agr-noun-adj = Agreement error
    .desc = The adjective {$1} should agree with the noun {$2}.

msyn-verb-form = Wrong verb form
    .desc = After "has", use the past participle "gone" not {$1}.
```

### Parameters

- `{$1}` - Error word (always available)
- `{$2}`, `{$3}` - Context words from CG3 relations
- `€1`, `€2` - Suggestions (replaced with actual values)

### Multi-Locale Example

**errors-en.ftl**:
```fluent
spelling-error = Spelling error
    .desc = The word {$1} is not in the dictionary.
```

**errors-fo.ftl**:
```fluent
spelling-error = Stavavillufeilur
    .desc = Orðið {$1} er ikki í orðabókini.
```

Select locale at runtime:
```bash
divvun-runtime run -c 'suggest={"locales":["fo","en"]}' bundle.drb "text"
```

## Complete Workflow

1. **Tag error in CG3**:
   ```cg3
   ADD (&typo) ? ;
   ```

2. **Map in errors.json**:
   ```json
   {
     "typo": [{ "id": "spelling-error" }]
   }
   ```

3. **Create message in .ftl**:
   ```fluent
   spelling-error = Spelling error
       .desc = The word {$1} is not in the dictionary.
   ```

4. **Test**:
   ```bash
   divvun-runtime run bundle.drb "mispeled word"
   ```

Output:
```json
{
  "form": "mispeled",
  "beg": 0,
  "end": 8,
  "err": "typo",
  "msg": ["Spelling error", "The word mispeled is not in the dictionary."],
  "rep": ["misspelled"]
}
```

## Error Tag Naming

Use consistent prefixes:

- `typo` - Spelling errors
- `msyn-*` - Morphosyntactic errors
- `lex-*` - Lexical selection
- `agr-*` - Agreement errors
- `real-word-error` - Wrong word (correct spelling)

## File Structure

```
assets/
├── errors.json
├── errors-en.ftl
├── errors-fo.ftl
└── errors-sma.ftl
```

## Common Patterns

### Spelling

```fluent
typo = Spelling error
    .desc = The word {$1} is not in the dictionary. Try €1.
```

### Grammar

```fluent
agr-subj-verb = Subject-verb agreement
    .desc = The verb {$1} should agree with the subject.
```

### Lexical

```fluent
wrong-word = Wrong word
    .desc = Use €1 instead of {$1} in this context.
```


## Error Tags in CG3

CG3 rules tag errors using `&` prefix:

```cg3
ADD (&typo) unknown-word ;
ADD (&agr-subj-verb) verb-with-wrong-agreement ;
```

### COERROR Tags

Related errors use `co&` prefix:

```cg3
ADD (&agr-noun-adj) Adj IF (1 N + Sg) (0 Adj + Pl) ;
ADD (co&agr-noun-adj) N IF (-1 Adj + (&agr-noun-adj)) ;
```
