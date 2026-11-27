# css-gpui

Design a component in HTML and CSS with an AI assistant, approve it in the browser, and turn it into readable Rust for [GPUI](https://www.gpui.rs/). The goal is native UI code a developer can build on. HTML supplies the element tree and text. CSS describes how that tree should look and respond to the space available.

## What works today

This repository starts with the Rust frontend from SEMUI's original Scene IR compiler. It parses a bounded HTML/CSS subset, resolves styles, extracts a typed scene, and can emit reference HTML/CSS. Eleven fixture inputs exercise layouts, typography, controls, and unsupported declarations or selectors. GPUI code generation is the first implementation milestone.

With Rust stable and Cargo installed, run the inherited tests and inspect the scene for a real component:

```sh
cargo test --locked
cargo run --locked --example inspect -- fixtures/v0.1/stacked_info_card/source.html fixtures/v0.1/stacked_info_card/source.css
```

The example writes the resolved scene as JSON to stdout and reports recognized unsupported constructs on stderr.

## Build the standalone toolbar

On Linux with X11 or XWayland, edit [the toolbar CSS](fixtures/toolbar/source.css) and run `python3 fixtures/toolbar/build.py` from the repository root. The command writes readable Rust and a runnable GPUI executable. See the [toolbar build instructions](docs/first-milestone.md#build-and-run-the-toolbar) for prerequisites and output paths.

## Project notes

- [Founding brief](docs/founding.md) — the authoring workflow and what makes the output worth keeping.
- [Compiler design](docs/compiler.md) — the imported code, its limits, and the boundary for GPUI generation.
- [First milestone](docs/first-milestone.md) — a resizable toolbar, the browser reference, and the conditions for calling the port successful.
- [Independent CSS compatibility benchmark](benchmarks/css-compat/README.md) — frozen inputs, browser and native comparisons, and measured feature coverage.
