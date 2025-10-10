# Text-to-Speech Overview

Text-to-speech (TTS) synthesis converts text to audio using neural models.

## Requirements

TTS requires the `mod-speech` feature:

```bash
export LIBTORCH=/opt/homebrew  # macOS
# or
export LIBTORCH=/opt/libtorch   # Linux

cargo build --features mod-speech
divvun-runtime sync
```

## Pipeline Flow

```
Text → Tokenize → Normalize → Phonology → Sentences → TTS → Audio
```

1. **Tokenize** - Split text into morphological units (hfst.tokenize)
2. **Normalize** - Convert numbers, abbreviations to words (speech.normalize)
3. **Phonology** - Add phonological forms (speech.phon)
4. **Sentences** - Extract sentence strings (cg3.sentences)
5. **TTS** - Synthesize speech (speech.tts)

## Speech Modules

### speech.normalize

Normalize text for speech:

```typescript
let x = speech.normalize(input, {
    normalizers: { "Sem/Plc": "place-norm.hfst" },
    generator: "generator.hfst",
    analyzer: "analyzer.hfst"
});
```

Converts:
- Numbers to words (123 → "one hundred twenty-three")
- Abbreviations to full forms (Dr. → "Doctor")
- Special tags to appropriate forms

### speech.phon

Add phonological representations:

```typescript
let x = speech.phon(input, {
    model: "phon.hfst",
    tag_models: { "Prop": "phon-prop.hfst" }
});
```

Generates pronunciation forms for synthesis.

### speech.tts

Synthesize audio:

```typescript
let audio = speech.tts(sentences, {
    voice_model: "voice.onnx",
    univnet_model: "vocoder.onnx",
    speaker: 0,
    language: 0,
    alphabet: "sme"  // "sme", "smj", "sma", "smi"
});
```

Returns WAV audio bytes.

## Sentence Extraction

Extract sentences with phonological forms:

```typescript
let sentences = cg3.sentences(input, {
    mode: "phonological"
});
```

Returns array of sentence strings ready for TTS.

## Supported Languages

Current alphabet options:
- `sme` - Northern Sami
- `smj` - Lule Sami
- `sma` - Southern Sami
- `smi` - Generic Sami

## Audio Output

TTS returns bytes (WAV format). Save to file:

```bash
divvun-runtime run -o output.wav bundle.drb "text to speak"
```

Or process further in TypeScript.
