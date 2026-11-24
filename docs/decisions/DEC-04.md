+++
becomes = "history"
date = "2026-11-24"
evidence = ["ISS-01", "base:1c8b943bb59e1e588ebcbae53b5a19e530a1cf1c", "docs/compiler.md", "docs/first-milestone.md"]
id = "DEC-04"
revised_at = "2026-11-24T17:45:53.3570488Z"
scope = ["docs/compiler.md", "docs/first-milestone.md"]
status = "accepted"
superseded_by = []
supersedes = []
title = "Report source losses independently of target limitations"

[author]
agent = "shell"
person = "junovhs"
+++

# DEC-04: Report source losses independently of target limitations

## Context

Unsupported input can be discarded before an emitter sees the scene. Some input, including at-rules, can fail at parse time.

## Decision

Keep parse failures, recognized unsupported declarations or selectors, and target capability gaps distinguishable. A known unsupported declaration must identify the source and explain the loss; that conversion cannot be marked clean.

## Why

The author needs an actionable location, and a target cannot diagnose information already discarded upstream.

## Alternatives rejected

Treating a clean emitter run as proof of source fidelity conceals frontend losses.

## Consequences

Expand the bounded support contract one behavior and fixture at a time. Retain a deliberately unsupported case alongside successful examples.
