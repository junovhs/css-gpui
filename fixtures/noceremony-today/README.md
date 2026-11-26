# noceremony Today visual benchmark

[Browser reference](browser-reference.png) is a capture of the running
[noceremony app at a1375b8](https://github.com/junovhs/noceremony/tree/a1375b8a54f77f6ecdb18a24081980aab16c1190).
It uses the app's real HTML, CSS, fonts, icons, and built-in sample day.

## Reproduce the browser frame

At the pinned revision, install the app dependencies with `npm ci`. Run the app
locally with `npm run dev -- --host 127.0.0.1 --port 4176 --strictPort`. From
`css-gpui`, run:

```text
node fixtures/noceremony-today/capture.mjs <path-to-noceremony>
```

The capture uses Chrome 158.0.8127.36 on Windows, a 1280 × 900 viewport at
device scale 1, light mode, and 10:30 a.m. Pacific on November 25, 2026.
Each capture starts in a clean browser context and clicks **Load a fake day**.
The script requires two identical screenshots before saving the reference.

## Conversion status

The browser frame is the baseline for the future native comparison. Use the
same app revision, state, viewport, scale, and fonts when comparing generated
GPUI output. For a component-sized comparison, take the matching region from the
real browser frame, starting with the first task card (`#commandCard`), and
compare its layout and pixels against the native render. Record any visual
difference or unsupported declaration against the original source. A layout
check and a pixel comparison answer separate questions.

The adjacent `source.html` and `source.css` are a reduced compiler diagnostics
case. Their Rust test checks text order and reports unsupported CSS Grid. The
native conversion and visual comparison are pending.

## Compile the real app's fixed browser frame

Pass an output directory and monitor scale to capture a browser-resolved
Scene IR and its browser PNG in the same run:

```text
node fixtures/noceremony-today/capture.mjs <path-to-noceremony> http://127.0.0.1:4176/ target/today 1.25
cargo run --locked --example generate_gpui -- --scene-json target/today/scene.json
```

This path reads the app's live DOM and computed styles after its own JavaScript
loads the sample day. Browser layout determines the fixed coordinates. GPUI
receives native rectangles, borders, and text. The reference PNG is saved
separately and is never passed to the generator. `losses.json` identifies
unrepresented CSS effects and elements by DOM path, including the SVG artwork
and layered backgrounds. Downloaded font files are collected locally for a
native host to load.

The direct source compiler currently stops at the bare `crossorigin` attribute
in noceremony's original `index.html`. The browser-resolved path allows a visual
comparison for one viewport while work on direct, responsive conversion remains
open.
