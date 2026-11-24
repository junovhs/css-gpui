+++
becomes = "history"
date = "2026-11-24"
evidence = ["ISS-01", "base:1c8b943bb59e1e588ebcbae53b5a19e530a1cf1c", "docs/founding.md"]
id = "DEC-01"
revised_at = "2026-11-24T17:45:52.6226225Z"
scope = ["docs/founding.md"]
status = "accepted"
superseded_by = []
supersedes = []
title = "Keep AI in authoring and application behavior in Rust"

[author]
agent = "shell"
person = "junovhs"
+++

# DEC-01: Keep AI in authoring and application behavior in Rust

## Context

CSS prototypes can already be approved in a browser, but developers reconstruct them for native use.

## Decision

Convert approved static HTML/CSS deterministically without a model or API key. Ship GPUI-rendered Rust that callers can edit, keep or stop regenerating. Callers supply data and actions.

## Why

A developer can review design changes as source diffs and retain ownership of application behavior.

## Alternatives rejected

Model-assisted conversion would compromise reproducibility. Shipping the preview in a browser would miss the native target. Inferring an action from a button label lacks application context.

## Consequences

Keep the approved source alongside generated code. JavaScript, state inference and whole-site migration are outside the first milestone.
