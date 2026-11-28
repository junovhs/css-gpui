# Backrooms strict conversion benchmark

On November 27, 2026, `generate_gpui` at `8e2c52bea8bc6d1a3ec04ca49d4554c8dfeb1d74` parsed the full browser-served HTML and inline CSS for Ben Evans’s Backrooms Pen. Both strict runs stopped during source diagnostics with **5,102 source losses** and exit code `1`. The first reported loss was the unevaluated `@charset "UTF-8"` at `input.html:19:1`. The first responsive loss was the portrait media query at `input.html:33:1`. [Original Pen](https://codepen.io/ivorjetski/pen/NPqLLoz) · [Browser-served HTML](https://cdpn.io/pen/debug/NPqLLoz)

| Viewport | Earliest fatal stage | Source losses | GPUI target losses | Rust emitted |
| --- | --- | ---: | --- | --- |
| 390 × 844 | Source diagnostics | 5,102 | Stage blocked | 0 bytes |
| 1440 × 900 | Source diagnostics | 5,102 | Stage blocked | 0 bytes |

These viewports cover portrait and landscape and fall on opposite sides of the Pen’s 1,300 px breakpoint. The compiler reports `@media only screen and (min-width: 1300px)` at `input.html:9101:1` as unevaluated at both widths. The two stderr streams are byte-identical with SHA-256 `7d8febc13844b28b1e300aaf48f51dba0c770b7f761998d4e3e719be36d131bf`. Every source location below refers to the pinned HTML response, including its inline stylesheet. [Browser-served HTML](https://cdpn.io/pen/debug/NPqLLoz)

| Capability | Diagnostics per viewport | Reported examples in the source |
| --- | ---: | --- |
| CSS selectors | 4,559 | 4,330 static selector losses involving `:has()`, first at `input.html:39:1`. The other 227 static selector losses include pseudo-elements at `input.html:75:1` and checked sibling selectors at `input.html:139:1`. Two selectors reach elements outside the generated interaction subset, first at `input.html:7911:1`. |
| CSS properties dropped | 426 | 160 `transform` at `input.html:101:3`, eight `transform-origin` at `input.html:2004:3`, five `backface-visibility` at `input.html:85:3`, two `perspective` at `input.html:98:3` and one `transform-style` at `input.html:84:3`. Motion includes 31 `animation` at `input.html:5717:3` and seven `transition` at `input.html:7969:3`. Paint includes 28 `box-shadow` at `input.html:1951:3` and 17 `filter` at `input.html:2584:3`. |
| Unsupported values or state changes | 33 | 24 `display` values, first `display: none` at `input.html:53:3`. Eight interactive changes to `transform` or `box-shadow`, first at `input.html:7914:3`. One interactive background value at `input.html:8886:3`. |
| CSS at-rules | 27 | 21 `@keyframes` at `input.html:3333:1`, four `@media` at `input.html:33:1`, one `@charset` at `input.html:19:1` and one `@-moz-document` at `input.html:9973:1`. |
| HTML form controls | 57 | 40 radios require `appearance: none` at `input.html:13962:5`, two checkboxes at `input.html:13961:5`, ten unresolved label bindings at `input.html:14192:7` and five unsupported radio label associations at `input.html:14087:21`. |

Counts are emitted diagnostic records, including repeated occurrences. The locations are the first reported for each example. The source analyzer measures its CSS and form-control subset and skips the bodies of unevaluated at-rules. The strict exit blocks GPUI target analysis, native build, pixel and layout comparison, and interaction replay. The compiler’s static input check leaves JavaScript and media behavior unmeasured.

## Reproduce

From the repository root with Python 3, curl and the project Rust toolchain installed, run the following command. It prints both complete diagnostic groupings as JSON. The optional `--logs /tmp/backrooms-diagnostics` retains the compiler's raw stderr for each viewport locally.

```sh
python3 benchmarks/backrooms/run.py
```

The runner checks both fingerprints, puts the full response in a temporary directory, and invokes the ordinary `generate_gpui --viewport WIDTHxHEIGHT input.html linked.css` command. `linked.css` is empty because the page provides its stylesheet inline. The runner verifies the CSS bytes without extracting them into a second compiler input. A changed response fails the fingerprint check before the compiler runs.

| Input | Bytes | SHA-256 |
| --- | ---: | --- |
| Browser-served HTML | 488,650 | `373084e9cf19f2a62b072eed146a687d2879eac74fa1374b0b3ef865836e1c49` |
| Inline CSS, including surrounding whitespace | 462,883 | `4b51d48812b2482e70e43501ff00811cd92b2a562e7b8bd7c0e839b98c072354` |

## Radio-selection snapshots

Run the differential snapshot probe with Node.js, curl, the Rust toolchain and
a local Playwright 1.60.0 installation with Chromium installed.

```sh
node benchmarks/backrooms/snapshot.mjs /path/to/playwright-install
```

The probe downloads the same pinned HTML to a temporary directory and compares
`inspect --radio-snapshot` with Chromium at both viewports. It feeds the
whole document to each implementation. The browser probe blocks the Pen's
scripts and external resources, disables animation and transitions, and sets
the checked property on the original inputs. It compares resting CSS values
directly. For camera samples, it converts the resolver's authored `rem`
translations and `rotateY` angle to the units returned by Chromium.

| Radio selection | Menu offset X | Walk offset Z | Turn | Movement label targets | Background | Level-two label |
| --- | ---: | ---: | ---: | --- | --- | --- |
| Authored defaults | 480 px | −408 px | 0° | z-8, left1, z-10, right1 | `#6f6d39` | block |
| Enter | 0 px | −408 px | 0° | z-8, left1, z-10, right1 | `#6f6d39` | block |
| Walk to z-8 | 0 px | −360 px | 0° | z-7, left1, z-9, right1 | `#6f6d39` | block |
| Turn right | 0 px | −360 px | 90° | x-6, left4, x-8, right2 | `#6f6d39` | block |
| Select level two | 0 px | −360 px | 90° | x-6, left4, x-8, right2 | `#6f3939` | none |

All ten comparisons passed, including the ordered movement-label classes and
targets, the start overlay changing from static to absolute when entering, and
the level-two label's legacy `:before` content. The pinned CSS
contains 10,622 flat `:has()` calls. Each includes `:checked`. The
snapshot exercises checked sibling selectors, compound and negated
`:has()`, and a grouped override that replaces the authored default.
Snapshot results establish cascade parity for these samples. Native label
activation, media-dependent sizing, full 3D projection and frame comparison
for Backrooms remain open. Strict generation continues to report unsupported
live relational state.

## Corner projection snapshots

```sh
node benchmarks/backrooms/projection.mjs /path/to/playwright-install
```

The projection probe uses the same pinned document and Playwright setup. At
390 × 844 and 1440 × 900 it samples the floor, tiles in columns 6–9 and rows
6–10, their spans, and their generated faces after entering, walking to z-8,
turning right, and selecting level two. Those states use the original radio
IDs. The Rust resolver reads the original cascaded transforms and handles
the portrait media query that changes the camera scale. The projector
composes the nested transform lists, transform origins and parent perspective
in a continuous `preserve-3d` context.

Chromium supplies the untransformed border boxes to Rust as layout inputs,
including the positions produced by the original floor's changing flex wrap.
A separate Chromium DevTools measurement of the original transformed border
quads supplies the reference. Across eight snapshots the probe compared
1,372 finite vertices, including 436 on wall faces, and 282 viewport polygons
for boxes whose four corners were in front of the eye. Of those polygons,
106 came from wall faces. The maximum coordinate error was 0.00224 CSS px
against a 0.035 CSS px tolerance.

Another 218 sampled boxes had at least one vertex on or behind the eye.
The probe compares their finite vertices and checks the output stays in the
viewport. Twenty-nine of those boxes retained a viewport polygon. The Rust
test `a_face_crossing_the_eye_is_clipped_before_perspective_division` checks
the exact clipped outline of an independent wall crossing the eye.

This isolates projected geometry. Standalone layout still needs a native
source of the responsive, wrapped border boxes. Rendering still needs backface
culling, visibility and depth ordering, authored paints, and motion. Live
navigation and browser/native frame comparison remain open. At both viewports,
ordinary strict GPUI conversion still exits at the source diagnostic stage
with 5,102 losses and emits zero bytes of Rust.
