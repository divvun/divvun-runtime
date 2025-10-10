# Modules & Commands

## hfst

Morphological analysis with finite state transducers.

??? abstract "tokenize"
    Tokenize text using PMHFST model.

    ```typescript
    let x = hfst.tokenize(entry, {
        model_path: "tokeniser.pmhfst"
    });
    ```

    **Input**: String | **Output**: String (CG3 format)

## cg3

Constraint Grammar disambiguation and processing.

??? abstract "vislcg3"
    Apply CG3 rules from compiled grammar.

    ```typescript
    let x = cg3.vislcg3(input, {
        model_path: "grammar.bin",
        config: { trace: false }
    });
    ```

    **Input**: String (CG3) | **Output**: String (CG3)

    !!! tip
        Enable tracing: `-c 'vislcg3={"config":{"trace":true}}'`

??? abstract "mwesplit"
    Split multi-word expressions.

    ```typescript
    let x = cg3.mwesplit(input);
    ```

    **Input**: String (CG3) | **Output**: String (CG3)

??? abstract "streamcmd"
    Insert CG3 stream commands (SETVAR, REMVAR).

    ```typescript
    let x = cg3.streamcmd(input, { key: "SETVAR" });
    ```

    **Input**: String (CG3) | **Output**: String (CG3)

    !!! tip
        Configure: `-c 'cmd-id="variable=value"'`

??? abstract "sentences"
    Extract sentences from CG3 stream.

    ```typescript
    let sentences = cg3.sentences(input, {
        mode: "surface"  // or "phonological" for TTS
    });
    ```

    **Input**: String (CG3) | **Output**: ArrayString

??? abstract "to_json"
    Convert CG3 output to JSON.

    ```typescript
    let json = cg3.to_json(input);
    ```

    **Input**: String (CG3) | **Output**: Json

## divvun

Spell/grammar checking and suggestions.

??? abstract "blanktag"
    Analyze whitespace using HFST.

    ```typescript
    let x = divvun.blanktag(input, {
        model_path: "analyser-gt-whitespace.hfst"
    });
    ```

    **Input**: String (CG3) | **Output**: String (CG3)

??? abstract "cgspell"
    Spell check with error models.

    ```typescript
    let x = divvun.cgspell(input, {
        err_model_path: "errmodel.hfst",
        acc_model_path: "acceptor.hfst",
        config: {
            n_best: 10,
            max_weight: 5000.0,
            beam: 15.0,
            recase: true
        }
    });
    ```

    **Input**: String (CG3) | **Output**: String (CG3 with suggestions)

??? abstract "suggest"
    Generate error report with suggestions.

    ```typescript
    let errors = divvun.suggest(input, {
        model_path: "generator.hfstol"
    });
    ```

    **Input**: String (CG3 with error tags) | **Output**: Json (error array)

    !!! tip
        Configure locales and filters: `-c 'suggest={"locales":["fo","en"],"ignore":["typo"]}'`

## speech

Text-to-speech synthesis.

!!! note
    Speech features must be enabled during build.

??? abstract "normalize"
    Normalize text for TTS.

    ```typescript
    let x = speech.normalize(input, {
        normalizers: { "Sem/Plc": "place-norm.hfst" },
        generator: "generator.hfst",
        analyzer: "analyzer.hfst"
    });
    ```

    **Input**: String (CG3) | **Output**: String (CG3 with phonological forms)

??? abstract "phon"
    Add phonological forms.

    ```typescript
    let x = speech.phon(input, {
        model: "phon.hfst",
        tag_models: { "Prop": "phon-prop.hfst" }
    });
    ```

    **Input**: String (CG3) | **Output**: String (CG3 with phon tags)

??? abstract "tts"
    Synthesize speech.

    ```typescript
    let audio = speech.tts(sentences, {
        voice_model: "voice.onnx",
        univnet_model: "vocoder.onnx",
        speaker: 0,
        language: 0,
        alphabet: "sme"  // "sme", "smj", "sma", "smi"
    });
    ```

    **Input**: String or ArrayString | **Output**: Bytes (WAV audio)

    !!! tip
        Override speaker: `-c 'tts-cmd={"speaker":1}'`

## example

Learning and demo functions.

??? abstract "reverse"
    Reverse a string.

    ```typescript
    let x = example.reverse(entry);
    ```

??? abstract "upper"
    Convert to uppercase.

    ```typescript
    let x = example.upper(entry);
    ```
