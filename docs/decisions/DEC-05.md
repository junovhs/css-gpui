+++
becomes = "history"
date = "2026-11-24"
evidence = ["ISS-01", "base:1c8b943bb59e1e588ebcbae53b5a19e530a1cf1c", "docs/compiler.md", "docs/first-milestone.md"]
id = "DEC-05"
revised_at = "2026-11-24T17:45:53.6034713Z"
scope = ["docs/compiler.md", "docs/first-milestone.md"]
status = "accepted"
superseded_by = []
supersedes = []
title = "Judge native fidelity against the approved browser source"

[author]
agent = "shell"
person = "junovhs"
+++

# DEC-05: Judge native fidelity against the approved browser source

## Context

Internal scene round trips missed browser defaults such as button borders and padding.

## Decision

Compare matched element bounds and images from approved HTML/CSS and native GPUI under the same font, scale and viewport. Start with the proposed one-logical-pixel tolerance per edge. Review text rasterization separately.

## Why

Independent renders expose visible differences that serialization and the reference emitter cannot measure.

## Alternatives rejected

Schema round trips and imported fixtures establish the frontend baseline but cannot establish native placement.

## Consequences

Retain both images, bounds, source, generated Rust and comparison results. Document any tolerance change with a failing case.
