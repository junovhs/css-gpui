+++
becomes = "history"
date = "2026-11-24"
evidence = ["ISS-01", "base:1c8b943bb59e1e588ebcbae53b5a19e530a1cf1c", "docs/compiler.md", "docs/first-milestone.md"]
id = "DEC-03"
revised_at = "2026-11-24T17:45:53.110688Z"
scope = ["docs/compiler.md", "docs/first-milestone.md"]
status = "accepted"
superseded_by = []
supersedes = []
title = "Preserve flex relationships and unconstrained dimensions"

[author]
agent = "shell"
person = "junovhs"
+++

# DEC-03: Preserve flex relationships and unconstrained dimensions

## Context

Declared pixel geometry and space distributed by a parent have different meanings when width or content changes.

## Decision

Retain explicit lengths, propagate SpaceBetween through diagnostics, extraction, serialization and both emitters, and leave absent intrinsic widths unconstrained.

## Why

The title and action must keep their insets and alignment under a resize or a change of copy.

## Alternatives rejected

Capturing the browser coordinates for each viewport would reproduce individual snapshots while losing the layout rule.

## Consequences

Inspect the generated Rust for a native flex relationship and exercise both titles at both widths.
