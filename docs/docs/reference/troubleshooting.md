# Troubleshooting

Common issues and solutions.

## Type Checking Issues

### Module not found

**Error**:
```
error: Cannot find module './.divvun-rt/hfst.ts'
```

**Solution**: Run sync to generate types:
```bash
divvun-runtime sync
```

### Wrong command signature

**Error**:
```
error: Property 'model_path' is missing
```

**Solution**: Check command parameters in [Modules & Commands](../modules.md) or generated `.divvun-rt/` types.

## Runtime Issues

### File not found

**Error**:
```
Error: Asset not found: tokeniser.pmhfst
```

**Solution**: Verify file exists in `assets/` directory:
```bash
ls assets/tokeniser.pmhfst
```

### Message not found

**Error**:
```
Warning: Message ID 'typo' not found in locale 'en'
```

**Solution**: Add message to `errors-en.ftl`:
```fluent
typo = Spelling error
    .desc = The word {$1} is not in the dictionary.
```

### Wrong locale messages

Messages appear in wrong language.

**Solution**: Set locale priority:
```bash
divvun-runtime run -c 'suggest={"locales":["fo","en"]}' bundle.drb "text"
```

### Pipeline not found

**Error**:
```
Error: Pipeline 'spellOnly' not found
```

**Solution**: List available pipelines:
```bash
divvun-runtime list bundle.drb
```

Use exact pipeline name:
```bash
divvun-runtime run --pipeline spell-only bundle.drb "text"
```

## Debug Logging

Enable detailed logging:

```bash
RUST_LOG=divvun_runtime=debug divvun-runtime run bundle.drb "input"
```

Module-specific logging:
```bash
export RUST_LOG=divvun_runtime::modules::cg3=trace
```

## Getting Help

If issues persist:

1. Check [Configuration Reference](./configuration.md)
2. Review [CLI Reference](../cli.md)
3. Search GitHub issues
4. File a bug report with:
   - Command used
   - Full error message
   - Platform (macOS/Linux)
   - Rust version (`rustc --version`)
