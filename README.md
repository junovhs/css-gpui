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

To generate a fixed-size screen snapshot, pass `--viewport WIDTHxHEIGHT` to
`generate_gpui`. The compiler evaluates `screen` and `only screen` queries,
portrait and landscape orientation, and pixel-based minimum and maximum width
and height, including conditions joined with `and`. Strict generation reports
unevaluated queries at their source locations.

Strict generation also supports a positive CSS `aspect-ratio` in block
flow and for absolute boxes with top and left offsets and a directly
positioned parent. The box must have one resolved dimension, one auto
dimension, and no in-flow content or min/max size constraints. GPUI
generation accounts for padding and a supported solid border using the
authored `box-sizing`. See the [browser and native bounds probe](benchmarks/css-compat/README.md#aspect-ratio-probe).

Strict generation also accepts physical `inset` shorthand and `right` and
`bottom` offsets on supported left-to-right absolute and fixed boxes. Offsets
accept `auto`, zero, and signed lengths in px, em, rem, or supported viewport
units. On each axis, the box needs a resolved size and a non-auto edge, or an
unconstrained auto size with both edges set. Absolute boxes use a directly
positioned block or flex parent or the initial viewport containing block. See
the [physical inset probe](benchmarks/css-compat/README.md#physical-inset-probe).

## Build the standalone toolbar

On Linux with X11 or XWayland, edit [the toolbar CSS](fixtures/toolbar/source.css) and run `python3 fixtures/toolbar/build.py` from the repository root. The command writes readable Rust and a runnable GPUI executable. See the [toolbar build instructions](docs/first-milestone.md#build-and-run-the-toolbar) for prerequisites and output paths.

## Build the standalone Neon Glass menu

On Linux with X11 or XWayland, edit the CSS in [the Neon Glass Pen](fixtures/neon-glass/source.html) and run `python3 fixtures/neon-glass/build.py "$browser"`. See the [Neon Glass build instructions](fixtures/neon-glass/README.md#build-the-local-menu-on-linux) to set up the pinned browser tools and find the generated Rust and executable.

## Play the Backrooms source preview

On Linux with X11 or XWayland, run `python3 benchmarks/backrooms/play.py` from the repository root. The first run fetches the pinned source and builds the optimized GPUI preview. See the [local preview instructions](benchmarks/backrooms/README.md#play-the-local-source-preview) for dependencies, offline launch, and the current fidelity limits.

## Project notes

- [Founding brief](docs/founding.md) — the authoring workflow and what makes the output worth keeping.
- [Compiler design](docs/compiler.md) — the imported code, its limits, and the boundary for GPUI generation.
- [First milestone](docs/first-milestone.md) — a resizable toolbar, the browser reference, and the conditions for calling the port successful.
- [Independent CSS compatibility benchmark](benchmarks/css-compat/README.md) — frozen inputs, browser and native comparisons, and measured feature coverage.
