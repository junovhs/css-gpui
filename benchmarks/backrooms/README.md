# Backrooms strict conversion benchmark

On November 27, 2026, `generate_gpui` at `8e2c52bea8bc6d1a3ec04ca49d4554c8dfeb1d74` parsed the full browser-served HTML and inline CSS for Ben Evans’s Backrooms Pen. Both strict runs stopped during source diagnostics with **5,102 source losses** and exit code `1`. The first reported loss was the unevaluated `@charset "UTF-8"` at `input.html:19:1`. The first responsive loss was the portrait media query at `input.html:33:1`. [Original Pen](https://codepen.io/ivorjetski/pen/NPqLLoz) · [Browser-served HTML](https://cdpn.io/pen/debug/NPqLLoz)

Evans published the compiled CSS because the full Sass source exceeded
CodePen’s size limit. The benchmark pins that published output. The Pen
does not supply the complete Sass or identify the historical compiler
version. The upstream input is fetched for local verification, and the
repository contains the benchmark code. [Author’s write-up](https://dev.to/ivorjetski/front-end-backrooms-css-1fkf)

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

## Play the local source preview

On Linux with X11 or XWayland, install curl and the
[Linux native build and Vulkan dependencies](../../docs/first-milestone.md#build-and-run-the-toolbar),
including Python 3 and Rust 1.95.0 through rustup. From the repository root,
run:

```sh
python3 benchmarks/backrooms/play.py
```

The first run downloads the pinned original HTML, checks the HTML and inline
CSS fingerprints, and builds the GPUI viewer in release mode. The local
source and binary are stored in `target/backrooms`. Click Enter and use the
control strip below the scene to move and turn. Click Show workings out to
reveal Top view. The default scene is 1152 × 648 plus the 48 px strip. Pass
`--viewport 1440x900` to sample the larger authored layout on a display with
room for the strip.

Add `--prepare-only` to fetch and build without opening a window. On later
runs, `python3 benchmarks/backrooms/play.py --offline` verifies the cached
source and opens the prepared app directly. Offline launch needs Python and
the native runtime dependencies. The preview runs with the browser closed
and no network access.

The native strip exposes actions from the authored source, including the path
to Finish. In-scene controls and HUD hit testing, the title treatment, audio,
and the Enter, wraparound-turn and ending keyframes still lack native support.
Intermediate motion is capped at a 640 × 360 raster at the default viewport.
The viewer settles at full resolution. The strict compiler still reports
5,102 source losses and emits zero Rust bytes at either benchmark viewport.

## Reproduce the strict benchmark

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
Snapshot results establish cascade parity for these samples. Strict generation
continues to report unsupported live relational state.

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

Rust derives used border boxes from the authored cascade, including the
centered absolute camera, percentage sizes, visible tiles in the wrapping
floor, and absolutely positioned generated faces. The probe sends only
the sampled DOM paths to Rust. Chromium DevTools independently measures
untransformed border boxes and transformed border quads for comparison.
Across eight snapshots, all 548 border boxes matched within 0.000031 CSS px
against a 1/64 CSS px tolerance. Projection from the Rust boxes matched
1,372 finite vertices, including 436 on wall faces, and 282 viewport polygons
for boxes whose four corners were in front of the eye. Of those polygons,
106 came from wall faces. The maximum coordinate error was 0.00224 CSS px
against a 0.035 CSS px tolerance.

Another 218 sampled boxes had at least one vertex on or behind the eye.
The probe compares their finite vertices and checks the output stays in the
viewport. Twenty-nine of those boxes retained a viewport polygon. The Rust
test `a_face_crossing_the_eye_is_clipped_before_perspective_division` checks
the exact clipped outline of an independent wall crossing the eye.

The bounded layout covers unbordered absolute boxes with explicit insets and
sized flex items with row wrapping and start or end line packing. It uses a
settled viewport and the authored cascade. The preview and replay below use
those boxes for drawing and input verification. At both viewports, ordinary
strict GPUI conversion exits at the source diagnostic stage with 5,102
losses and emits zero bytes of Rust.

## Settled GPUI corner preview

The planar preview draws the original `content` subtree at the authored enter,
walk and right-turn radio selections. Rust resolves its styles and box layout,
projects element and generated faces through CSS perspective, and clips to the
eye plane and viewport. It samples authored solid colors and supported gradient
layers in local plane coordinates. It culls hidden backfaces and composites
overlapping fragments by depth at each pixel. The renderer then samples
viewport-covering ancestor pseudo-elements that stack above the scene and
applies `mix-blend-mode: overlay` to the opaque canvas. It supports flat,
unisolated ancestor chains and centered farthest-corner elliptical gradients.
The Backrooms stylesheet supplies the high-graphics layer on
`body > div::before`. GPUI uploads the Rust raster as a texture, and the probe
checks every RGBA byte in the actual X11 window against the upload.

The probe captures the authored Chromium page and GPUI scene at the same pose.
It captures both again with only the viewport gradient hidden in the authored
cascade. The isolated reference also hides paint outside the context while
preserving scene materials, filters, shadows and the backing. The coverage
reference recolors tile pseudo-elements red and green without changing their
transforms. Native and Chromium both render the same coverage stylesheet.

| Viewport and state | Full-page RGB error before | Full-page RGB error after | Isolated RGB error | Red coverage IoU | Green coverage IoU |
| --- | ---: | ---: | ---: | ---: | ---: |
| 390 × 844 enter | 42.13 | 10.72 | 12.31 | 0.527 | 0.925 |
| 390 × 844 walk | 44.07 | 7.59 | 9.10 | empty in both | 1.000 |
| 390 × 844 turn | 44.26 | 7.98 | 9.35 | 0.857 | 0.999 |
| 1440 × 900 enter | 44.71 | 5.41 | 6.96 | 0.640 | 0.865 |
| 1440 × 900 walk | 44.67 | 4.33 | 5.37 | 1.000 | 1.000 |
| 1440 × 900 turn | 45.12 | 3.77 | 5.51 | 0.686 | 0.935 |

RGB error is the mean absolute channel difference on a 0–255 scale. The two
full-page columns use the authored Chromium page as the reference. In the lighting-only comparison, hiding the gradient reproduced the six
earlier native readbacks byte for byte. The probe checks that the layer reduces full-page error by at least 25.
It also compares the signed RGB change from enabling the layer in the upper
left, center, and lower right of the viewport. Coverage
IoU is pixel intersection divided by union, with a threshold of 128 on the
corresponding channel. Red is `::before` and green is `::after` for the corner
sample. The lighting comparison contained 79–155 projected painted planes, including
some that were occluded. The isolated comparison caps RGB error at 13 and checks
green IoU of at least 0.85 and red IoU of at least 0.50 when present. These are
bounded preview regression thresholds. The reported coverage gaps are measured
raster differences. Native and isolated Chromium images both change between
successive states by at least 20% of pixels at an 8-level channel threshold.

The preview records per-plane losses for unsupported wallpaper gradients,
gradient stops, outlines, shadows, blur, bounded layout failures and group
overflow. The start screen adds inset shadows to the
viewport layer, so that layer remains a paint loss in the start state.
Differences in grouped compositing, occlusion, and edge sampling remain visible
in the coverage and paint comparisons. The HUD, motion interpolation, and
audio remain outside the scene preview.
Ordinary strict GPUI generation still reports 5,102 losses and emits
zero bytes of Rust for each viewport.

## Native corner replay

The local route builds four resting frames from the pinned HTML and CSS at
each viewport. The compiler reads the checked groups, available explicit
radio labels, and the captions from their authored text or generated content.
The route selects a bounded set of poses using the Enter label, the forward
label after entry, and the right-turn label after walking.

The GPUI window places the controls in a separate strip below the scene. A
mouse or keyboard click uses the label’s current target to replace one
checked radio group and select a compiled frame. Controls with no destination
in the bundle are disabled. The supported path enters, walks forward to
`z-8`, can walk back to `z-9` and forward again, and turns right to
`right1`. The original Enter label’s inline audio handler is recorded in
the bundle as metadata. The browser and native replay both exercise the CSS
radio association with scripts and audio omitted.

The replay clicks the original labels in Chromium and sends X11 input to one
GPUI window per viewport. The portrait replay uses the keyboard for the turn.
It compares
the scoped available labels against Chromium, checks the selected radio
groups after each activation, and compares the GPUI scene pixels to the
selected Rust frame. It also checks that walking back restores the entry
frame, and that reclicking the old forward slot leaves the scene unchanged
once the newly bound target is outside the compiled route. Chromium and
native scene comparisons use the same paint and coverage thresholds above.

From the repository root on Linux with the Rust toolchain, the GPUI X11 and
Vulkan build dependencies, Xvfb, xdotool, ImageMagick, and the Playwright setup
above, run:

```sh
target_dir=$(mktemp -d)
cargo build --locked --manifest-path benchmarks/css-compat/native/Cargo.toml \
  --features planar-preview --bin css-gpui-planar-preview --target-dir "$target_dir"
export VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json
node benchmarks/backrooms/planar.mjs /path/to/playwright-install \
  "$target_dir/debug/css-gpui-planar-preview"
```

The last command accepts an optional output directory for PNG captures,
including the overlay-disabled counterfactual, the route bundle, RGBA frames,
input traces, and per-plane metadata. The scene crops exclude the separate
native strip. Chromium geometry and screenshots are used solely in the
comparison.

### Projected wallpaper

The preview resolves each supported gradient image against its own
`background-size`, `background-position`, and `background-repeat`.
Short lists cycle using the original image slots. Sizes and positions
resolve in the local painting box. Tile sampling supports repeat and
no-repeat, then composites the layers over the background color. The
preview supports centered elliptical radial layers, linear layers, and
repeating linear layers with unitless zero or percentage stops and a
positive period. It reports unsupported length stops, complex positions
and repeat modes, and non-default origins, clips, and attachments.

The authored foreground wall at the right turn has a 96 × 48 CSS-pixel
face with seven layers on 4.8 × 3.2 px tiles. Two repeating linear
gradients provide narrow strips. The projected Chromium crop becomes
sharper after removing only `brightness(1)` from this face. The filter
preserves color and changes how Chromium composites the projected
surface. The preview rasterizes brightness-filtered tiled materials at
one texel per CSS pixel and bilinearly samples the premultiplied texture
during projection. Each visible filtered face builds its own texture
during a settled render. Equivalent faces and successive renders
currently build their textures independently. In nine interleaved
right-turn runs of the release inspection command, median time with
analytic sampling and with local texturing was 861 / 818 ms at portrait
size and 1,229 / 1,044 ms at desktop size. This measured source resolution,
settled rendering, and the RGBA write together.

The corner probe captures both renderers with the authored wallpaper
and with only the wall background images removed. The crop holds
lighting, geometry and base wall colors fixed. All GPUI readbacks
match the Rust raster.

| Viewport | Crop bounds (left, top, right, bottom) | Chromium / GPUI wallpaper change | Signed change error before / after correction | Authored crop error with images omitted / before correction / after correction |
| --- | --- | ---: | ---: | ---: |
| 390 × 844 | 12, 65, 70, 385 | 2.20 / 2.08 | 1.64 / 0.86 | 1.76 / 2.10 / 1.73 |
| 1440 × 900 | 180, 150, 390, 465 | 2.32 / 1.88 | 1.63 / 1.07 | 1.89 / 2.03 / 1.76 |

Wallpaper change is the mean absolute channel difference between
enabled and disabled frames. Signed change error compares both
renderers' enabled-minus-disabled RGB values. The probe requires this
error to be below 55% of the Chromium change and the raw crop to improve
over omission. The probe also measures horizontal contrast in the
wallpaper's RGB response to track the softness of its vertical marks.

| Viewport | Chromium with filter | Chromium with filter off | GPUI with filter |
| --- | ---: | ---: | ---: |
| 390 × 844 | 0.909 | 2.207 | 0.860 |
| 1440 × 900 | 0.355 | 0.801 | 0.224 |

The portrait turn no longer reports the 27 authored radial wallpaper
losses. The six settled corner comparisons improve at both widths.

| Viewport | Enter full-page RGB error | Walk full-page RGB error | Turn full-page RGB error |
| --- | ---: | ---: | ---: |
| 390 × 844 | 10.72 → 6.94 | 7.59 → 6.27 | 7.98 → 6.68 |
| 1440 × 900 | 5.41 → 3.53 | 4.33 → 3.42 | 3.77 → 3.22 |

The optional output directory retains the `-turn-without-wall-images`
captures, the matching Rust RGBA frames and the Chromium
`-turn-without-wall-filter` capture.

To build and open the local route without Playwright, use the same native
binary and run:

```sh
route_dir=$(mktemp -d)
node benchmarks/backrooms/route.mjs "$route_dir"
"$target_dir/debug/css-gpui-planar-preview" --route "$route_dir/390x844-route.json"
```

## Start-to-finish route

Use the same Playwright and GPUI setup as the corner replay.

```sh
node benchmarks/backrooms/full-route.mjs /path/to/playwright-install \
  "$target_dir/debug/css-gpui-planar-preview"
```

The probe fetches the pinned document and discovers a route shared by both
viewports with a breadth-first search over checked radio groups. It looks at explicit labels in
the entry overlay, movement controls and scene. Chromium checks the foremost
pointer target at sampled points on each label and its descendants. Each
transition requires a hit in both viewports. The probe keeps authored high
graphics and the default debug settings throughout. The search caps the
candidate queue at 4,000 states and can take several minutes. The pinned run
examined 2,153 states, found 2,183 candidates, and reached Finish in 45 clicks.

The replay starts with a fresh authored document at each viewport. Every
transition sends a real mouse click to an original label and verifies all
checked radio groups. The Rust preview starts independently with its authored
defaults, resolves the whole input at each subsequent radio selection, and
checks that each browser navigation label has a matching cascade-resolved Rust
control. GPUI replays the same target sequence from its separate control strip
and verifies the entire X11 input trace and scene readback. Each GPUI scene is
compared with the full-page Chromium frame, including overlays and the HUD.
The benchmark reports per-frame RGB error and Rust paint losses.

The shared route enters at click 1, switches to level two at click 17
(`x-2`, `z-10`, `right1`), and activates the Finish door at click 45
(`x-1`, `z-6`, `left3`). Each viewport passed 45 Chromium clicks and 45
GPUI clicks. All 46 GPUI scene readbacks per viewport matched the Rust
rasters byte for byte.

The probe also edits the wall faces' background at the pose reached by
click 17 and rebuilds the Rust scene with the changed stylesheet. Each
edited X11 readback matched the new Rust raster byte for byte.

| Viewport | Post-Enter before lighting | Post-Enter after lighting | Post-Enter with wallpaper | Start before / after wallpaper | Settled Finish before / after wallpaper |
| --- | ---: | ---: | ---: | ---: | ---: |
| 390 × 844 | 39.30–52.34 | 2.24–10.72 | 2.16–7.51 | 48.91 / 51.56 | 5.32 / 5.28 |
| 1440 × 900 | 37.29–53.29 | 0.83–5.41 | 0.78–4.92 | 50.96 / 50.60 | 3.21 / 3.14 |

Post-Enter ranges include the 45 resting frames after each activation. RGB
error is the mean absolute channel difference against the full Chromium page
on a 0–255 scale. With wallpaper, 30 of the 45 post-Enter frames improve
in portrait and 32 improve on desktop. The high start error includes the
authored title and the viewport layer's extra inset shadows. At the
8-level channel threshold, the wall edit changes 43.5% of Chromium pixels
and 43.6% of GPUI pixels in portrait. On desktop it changes 70.7% and
77.4%, respectively.

The optional final argument retains local browser and GPUI captures, compiled
RGBA frames, the GPUI bundle and comparison data. To open the compiled route,
run the existing preview binary with `--route` and
`390x844-full-route.json` or `1440x900-full-route.json` from that directory.
The GPUI host reads the precompiled bundle and images on startup. The probe
serves the downloaded document to Chromium from memory and blocks external
requests. The settled comparison disables authored scripts, audio and
animation. The finish radio is checked by the original door label.

## Native Top view replay

The checked-state snapshot applies activations in order from the authored
defaults. A radio selects one group member and a checkbox independently
toggles. The route reads explicit labels and direct, cascade-visible checkboxes
from the source. Existing radio-only bundles can omit the checked checkbox IDs.

Using the same Playwright setup and GPUI binary as the corner replay, run:

```sh
node benchmarks/backrooms/top-view.mjs /path/to/playwright-install \
  "$target_dir/debug/css-gpui-planar-preview" /tmp/backrooms-top-view
```

The probe clicks the original Enter and Show workings out labels, then clicks
the original Top view checkbox twice. It replays those four activations via X11
in a single GPUI window at each size. Both runs retain the same radio groups
through the checkbox clicks and check the authored checkbox IDs at each step.
All five GPUI readbacks per size equal the corresponding Rust rasters. Turning
Top view off restores the previous view byte for byte in Chromium and GPUI.

Rust resolves the overhead camera, floor boxes and generated wall geometry
from the authored cascade. The 687 painted planes at each size include the
maze walls and floor. The preview paints the authored viewport-covering white
body backing and composites the high-graphics viewport layer. CDP supplies
comparison geometry. The isolated reference preserves scene materials and the
backing while hiding the surrounding controls and the viewport layer.

| Viewport | Used boxes | Projected vertices (wall vertices) | Maximum box / vertex error (CSS px) | Full-page / isolated RGB error |
| --- | ---: | ---: | ---: | ---: |
| 390 × 844 | 65 | 236 (92) | 0.000031 / 0.000048 | 21.06 / 3.14 |
| 1440 × 900 | 65 | 236 (92) | 0 / 0.000034 | 8.24 / 3.36 |

The overhead comparison allows up to 1/64 CSS px of used-box error,
0.035 CSS px of vertex error, and 30/6 RGB error in the full-page/isolated
images. The output includes per-frame paint and layout losses. The optional
last argument keeps the screenshots, RGBA files, route bundles, and native
input traces. The GPUI control strip uses the route's compiled destinations.

The authored HUD, debug text inside the maze, grouped depth compositing and
animated ending remain open on ISS-22. Top view still reports unsupported
gradient variants, grouped opacity, outlines, generated content, and layout
for bordered or aspect-ratio decorations. Strict GPUI conversion reports 5,102
source losses and emits zero bytes of Rust at either viewport.

## Live native document preview

Open saved local HTML and CSS directly in the GPUI preview. Use an empty CSS
file when the document supplies its stylesheet inline. The final two arguments
identify the painted subtree and the control scope. Each accepts a unique
element tag or a `#id`.

```sh
"$target_dir/debug/css-gpui-planar-preview" --document 390x844 \
  /tmp/backrooms/input.html /tmp/backrooms/linked.css content body
```

The viewer parses the two files at startup and resolves each activation
against the in-memory source graph. While it resolves the next cascade, the
window keeps displaying the current scene. For supported transform
transitions, the new controls become available during movement. Another
activation starts from the latest submitted camera pose. Unsupported motion
uses the full-resolution settled preview. Native execution uses the local
source and GPUI.

The strip exposes scoped radio labels and direct checkboxes supported by the
resolver. It applies cascade visibility and pointer eligibility. The
authored HUD, viewport hit testing and occlusion of controls still need
native support.

```sh
node benchmarks/backrooms/live-branch.mjs /path/to/playwright-install \
  "$target_dir/debug/css-gpui-planar-preview" /tmp/backrooms-live-branch
node benchmarks/backrooms/full-route.mjs /path/to/playwright-install \
  "$target_dir/debug/css-gpui-planar-preview" /tmp/backrooms-full-live --live
```

The branch probe uses real Chromium and X11 input at both benchmark
viewports. It advances from `z-8` to `z-7`, which is outside the compiled
corner route, returns to `z-8`, turns right, and toggles Top view on and off.
It sends a rapid second native click while opening the branch and checks that
only one move is applied. All nine native scene readbacks per viewport equal
independently generated Rust rasters, and the ordered native trace matches
the reference control state and targets. Returning to `z-8` and switching
Top view off restore the preceding scene pixel for pixel within each
renderer.

| Viewport | GPUI exact scene readbacks | Authored / isolated RGB error at `z-7` |
| --- | ---: | ---: |
| 390 × 844 | 9 | 5.98 / 3.98 |
| 1440 × 900 | 9 | 2.99 / 3.25 |

With `--live`, the start-to-finish probe uses the same direct-source viewer.
At each viewport it reaches Finish in 45 Chromium clicks and 45 GPUI
activations. All 46 native scene readbacks match the independent Rust rasters
byte for byte. The compiled `--route` command opens retained route bundles.


## Native camera transitions

The live document preview uses authored transform durations, delays, and
cubic Bézier easing for the supported single-function rotations and
translations. Each frame reprojects the geometry and resamples the scene
from a local transform. The preview reuses plane paint within a resolved
cascade and presents an exact full-resolution raster at the endpoint.
Portrait motion uses the 390 × 844 raster. Desktop motion uses a 640 × 400
raster drawn at 1440 × 900 while layout and projection stay in CSS pixels.

The motion probe uses Chromium and an isolated X11 window. It clicks the
authored forward and right labels in Chromium, pauses each CSS transition,
and reads its transform at the native sample times. Chromium moves the
walk from −408 to −360 CSS px in 800 ms with `ease-out`. The right turn
uses `cubic-bezier(0, 0.3, 0.6, 1.4)` over 800 ms, passes 90°, and settles
back to 90°. The probe disables Chromium keyframes for this comparison and
sends real pointer input to the native control strip and polls X11 window
pixels for visible changes. It captures the settled window at full size and
checks every RGBA byte against an independent Rust raster. It then turns
300 ms into the walk, verifies both moves begin, and checks the final
checked state and raster.

| Viewport | Move | Visible changes including endpoint | First / final change | Median / longest gap |
| --- | --- | ---: | ---: | ---: |
| 390 × 844 | Walk | 19 | 186 / 841 ms | 34.0 / 51.5 ms |
| 390 × 844 | Right turn | 19 | 172 / 859 ms | 33.7 / 72.6 ms |
| 1440 × 900 | Walk | 22 | 175 / 859 ms | 33.0 / 46.7 ms |
| 1440 × 900 | Right turn | 22 | 174 / 846 ms | 33.3 / 46.0 ms |

Xvfb with Mesa lavapipe, a Ryzen 9 3900X, and the native Cargo development
profile produced these timings. X11 polling averaged 6.2–11.4 ms. All four
endpoints matched exactly. The largest differences from Chromium at the
logged sample times were 0.00050 CSS px for walking and 0.00003° for turning.
An independent rerun showed 18–22 visible changes, median gaps of 34–35 ms,
and first updates after 169–209 ms. The timings reflect this X11 setup.

With the optimized local-play build on the same Xvfb and lavapipe setup,
the first visible updates came after 102–165 ms and median frame gaps were
31.8–33.9 ms across the same four movements. The probe found 22–24 visible
changes per movement and exact full-resolution endpoints. The earlier
response leaves more of each 800 ms transition visible, with cadence still
around 30 distinct updates per second.

Run with the same native binary and Playwright setup as above, plus a C
compiler, X11 and XTest development libraries and `pkg-config`.

```sh
xvfb-run -a -s '-screen 0 1600x1000x24' \
  node benchmarks/backrooms/motion.mjs /path/to/playwright-install \
  "$target_dir/debug/css-gpui-planar-preview"
```

The preview uses settled frames for perspective changes and unsupported
transition syntax. CSS keyframes, the authored Enter sequence, the
wraparound-turn keyframes, the animated ending, audio, and the authored HUD
remain outside this motion check. Interruption starts each supported track
from the last submitted pose with its authored duration. Chromium's
transition reversal and continued timelines for unchanged targets remain
unmeasured. Strict generation remains separate and stops at 5,102 source
losses and zero Rust bytes per viewport.
