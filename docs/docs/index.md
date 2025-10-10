# Introduction

Welcome to the Divvun Runtime Guide.

Divvun Runtime is a modular language processing pipeline system that enables you to build sophisticated natural language processing applications using TypeScript-defined pipelines compiled to an efficient Rust runtime.

## What is Divvun Runtime?

Divvun Runtime provides:

- **Graph-based Architecture**: Define pipelines as directed acyclic graphs (DAGs)
- **Modular Processing**: Composable modules for morphological analysis, grammar checking, and text-to-speech
- **Cross-platform Support**: Rust core with Swift bindings, CLI tools, and visual development environment
- **Bundle Distribution**: Package pipelines with assets into distributable `.drb` files

## Primary Use Cases

### [Grammar Checking](grammar/overview.md)
Build complete grammar checking systems that detect spelling and grammatical errors, providing contextual suggestions.

### [Text-to-Speech](tts/overview.md)
Create TTS systems that convert text to natural-sounding speech with phonological processing and normalization.

## Getting Help

- **Documentation**: You're reading it!
- **GitHub**: [github.com/divvun/divvun-runtime](https://github.com/divvun/divvun-runtime)
- **Issues**: Report bugs and request features on GitHub

Let's get started with [Installation](./installation.md).
