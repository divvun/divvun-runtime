# Complete TTS Example

Full text-to-speech pipeline from text to audio.

## Pipeline

```typescript
import { Command, StringEntry } from './.divvun-rt/mod.ts';
import * as hfst from './.divvun-rt/hfst.ts';
import * as cg3 from './.divvun-rt/cg3.ts';
import * as speech from './.divvun-rt/speech.ts';

export default function tts(entry: StringEntry): Command {
    // Tokenize
    let x = hfst.tokenize(entry, {
        model_path: "tokeniser.pmhfst"
    });

    // Apply grammar rules
    x = cg3.vislcg3(x, {
        model_path: "disambiguator.bin"
    });

    // Normalize for speech
    x = speech.normalize(x, {
        normalizers: {
            "Sem/Plc": "place-norm.hfst",
            "Sem/Obj": "obj-norm.hfst"
        },
        generator: "generator.hfst",
        analyzer: "analyzer.hfst"
    });

    // Add phonology
    x = speech.phon(x, {
        model: "phon.hfst",
        tag_models: {
            "Prop": "phon-prop.hfst"
        }
    });

    // Extract sentences
    let sentences = cg3.sentences(x, {
        mode: "phonological"
    });

    // Synthesize
    return speech.tts(sentences, {
        voice_model: "voice.onnx",
        univnet_model: "vocoder.onnx",
        speaker: 0,
        language: 0,
        alphabet: "sme"
    });
}
```

## Assets Structure

```
assets/
├── tokeniser.pmhfst
├── disambiguator.bin
├── place-norm.hfst
├── obj-norm.hfst
├── generator.hfst
├── analyzer.hfst
├── phon.hfst
├── phon-prop.hfst
├── voice.onnx
└── univnet.onnx
```

## Running

Generate audio file:

```bash
divvun-runtime run -o output.wav ./pipeline.ts "Text to synthesize"
```

Play audio:

```bash
afplay output.wav  # macOS
aplay output.wav   # Linux
```

## Configuration

Override speaker at runtime:

```bash
divvun-runtime run -c 'tts-cmd={"speaker":1}' -o output.wav bundle.drb "text"
```

## Testing

Test individual stages:

```typescript
// Test normalization only
export function normalize_dev(entry: StringEntry): Command {
    let x = hfst.tokenize(entry, {
        model_path: "tokeniser.pmhfst"
    });
    x = speech.normalize(x, {
        normalizers: { "Sem/Plc": "place-norm.hfst" },
        generator: "generator.hfst",
        analyzer: "analyzer.hfst"
    });
    return cg3.to_json(x);  // Output as JSON to inspect
}

// Test phonology
export function phon_dev(entry: StringEntry): Command {
    let x = hfst.tokenize(entry, {
        model_path: "tokeniser.pmhfst"
    });
    x = speech.phon(x, {
        model: "phon.hfst",
        tag_models: {}
    });
    return cg3.to_json(x);
}
```

Run dev pipelines:

```bash
divvun-runtime run --pipeline normalize_dev ./pipeline.ts "Dr. Smith visited 123 Main St."
divvun-runtime run --pipeline phon_dev ./pipeline.ts "hello world"
```

## Common Issues

### Missing libtorch

```
error: linking with `cc` failed
```

**Solution**: Install libtorch and set LIBTORCH:
```bash
export LIBTORCH=/opt/homebrew
cargo build --features mod-speech
```

### Wrong Alphabet

Audio sounds incorrect.

**Solution**: Verify alphabet matches your language:
```typescript
alphabet: "sme"  // Must match voice model
```

### Incomplete Normalization

Numbers or abbreviations not converted.

**Solution**: Add normalizer models for missing tags:
```typescript
normalizers: {
    "Sem/Plc": "place-norm.hfst",
    "Sem/Obj": "obj-norm.hfst",
    "Sem/Date": "date-norm.hfst"
}
```
