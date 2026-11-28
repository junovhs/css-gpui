+++
becomes = "history"
date = "2026-11-28"
evidence = ["DEC-02", "DEC-04", "ISS-58", "base:bde9f8d824df13482acec19f9574ba137c434ffd", "docs/compiler.md", "src/ir/mod.rs", "src/target/gpui.rs", "src/target/mod.rs"]
id = "DEC-06"
revised_at = "2026-11-28T06:55:49.519243275Z"
scope = ["docs/compiler.md", "src/source_graph/", "src/resolver/", "src/layout/", "src/paint/", "src/ir/", "src/target/"]
status = "accepted"
superseded_by = []
supersedes = []
title = "Keep the HTML and CSS compiler independent of the GPUI runtime"

[author]
agent = "shell"
person = "junovhs"
+++

# DEC-06: Keep the HTML and CSS compiler independent of the GPUI runtime

## Context

GPUI is the current native backend. The HTML and CSS compiler is the project's long-term investment, and embedded, spatial, or other runtimes may eventually consume its output.

## Decision

Keep CSS property and selector interpretation, cascade, layout and paint meaning, and supported animation semantics in shared compiler code. Carry that meaning through a runtime-independent scene contract. The GPUI adapter owns mapping the resolved scene to GPUI, native input, scheduling updates and frames, and presentation. DEC-02 defines the readable Rust output and pinned GPUI compatibility check. DEC-04 governs source losses and target capability gaps.

## Why

A shared meaning for authored CSS keeps fidelity work reusable across targets. A concrete GPUI implementation makes the contract testable against a real runtime today.

## Alternatives rejected

Putting CSS interpretation in GPUI would tie an authored design's behavior to one runtime. Implementing extra backends or generalizing the interface around hypothetical requirements would add work ahead of a concrete use case.

## Consequences

Extend the shared scene contract as concrete CSS support requires it. Keep GPUI types and lifecycle assumptions in the adapter and its host. Report unsupported source behavior and backend limitations at their respective stages. Pursue another backend when a concrete use case calls for one.
