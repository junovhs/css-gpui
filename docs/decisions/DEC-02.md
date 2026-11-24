+++
becomes = "history"
date = "2026-11-24"
evidence = ["ISS-01", "base:1c8b943bb59e1e588ebcbae53b5a19e530a1cf1c", "docs/compiler.md"]
id = "DEC-02"
revised_at = "2026-11-24T17:45:52.8673447Z"
scope = ["docs/compiler.md", "src/target/"]
status = "accepted"
superseded_by = []
supersedes = []
title = "Generate readable Rust through the Scene IR target interface"

[author]
agent = "shell"
person = "junovhs"
+++

# DEC-02: Generate readable Rust through the Scene IR target interface

## Context

The inherited compiler extracts a typed scene and already has an HTML reference emitter.

## Decision

Implement GPUI generation through TargetEmitter over resolved Scene IR. Pin the GPUI revision when the first emitter lands and compile its output in a minimal host.

## Why

Sharing the scene preserves the frontend contract. Compilation checks that emitted Rust works with the selected GPUI API.

## Alternatives rejected

Translating directly from source would duplicate frontend work. Comparing generated text alone would leave API compatibility untested.

## Consequences

Changes to the supported scene may affect both emitters. The first native example must compile against the pin.
