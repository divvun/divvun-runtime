# Configuration Reference

Runtime configuration options for commands and modules.

## Command Configuration

Commands accept configuration objects in TypeScript:

```typescript
let x = command(input, {
    required_param: "value",
    optional_config: {
        option1: true,
        option2: 42
    }
});
```

## Runtime Configuration

Override configuration when running bundles:

```bash
divvun-runtime run -c 'cmd-id={config}' bundle.drb "input"
```

### Syntax

**String**:
```bash
-c 'cmd-id="value"'
```

**JSON object**:
```bash
-c 'cmd-id={"key":"value","num":42}'
```

**Array**:
```bash
-c 'cmd-id=["val1","val2"]'
```

**Multiple configs**:
```bash
-c 'cmd1="val1"' -c 'cmd2={"key":"val2"}'
```

## Common Configurations

### suggest Module

**Locale selection**:
```bash
-c 'suggest={"locales":["fo","en"]}'
```

**Ignore error types**:
```bash
-c 'suggest={"ignore":["typo"]}'
```

**UTF-16 encoding**:
```bash
-c 'suggest={"encoding":"utf-16"}'
```

### cg3.vislcg3

**Enable tracing**:
```typescript
config: { trace: true }
```

Or at runtime:
```bash
-c 'vislcg3={"config":{"trace":true}}'
```

### divvun.cgspell

**Tuning parameters**:
```typescript
config: {
    n_best: 10,          // Max suggestions
    max_weight: 5000.0,  // Max edit distance
    beam: 15.0,          // Quality range
    recase: true         // Try case changes first
}
```

### speech.tts

**Speaker override**:
```bash
-c 'tts-cmd={"speaker":1}'
```

## Configuration Layers

Configuration merges in order:

1. **Default values** - Built-in defaults
2. **Pipeline config** - TypeScript configuration objects
3. **Runtime config** - CLI `-c` flags

Runtime config overrides pipeline config.
