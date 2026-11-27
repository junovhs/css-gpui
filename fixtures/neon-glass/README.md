# Neon Glass browser reference

Simey ([@simeydotme](https://codepen.io/simeydotme)) published [Neon Glass Context Menu](https://codepen.io/simeydotme/pen/PwwYZOe). On 2026-11-27 this fixture extracted the inner document from the [CodePen full-page renderer](https://cdpn.io/simeydotme/fullpage/PwwYZOe), with CodePen chrome references and trailing whitespace removed. The remaining markup, compiled CSS and script came from that renderer. The source hash uses LF line endings.

[CodePen licenses public Pens under MIT](https://blog.codepen.io/docs/pens/licensing/). The notice for the vendored Pen code appears below. The dependency URLs and SHA-256 hashes are in [reference.json](reference.json).

| Input | Provenance and redistribution |
| --- | --- |
| Shared UI CSS | Simey's [template css 2025 render](https://cdpn.io/simeydotme/fullpage/jEOaeNd/63809857734821de257097bf620e7121) provides the stylesheet referenced by the original Pen. The replay extracts and downloads it locally. Its separate publication and license status are unverified. |
| Normalize 5.0.0 | CodePen CDN copy of [Normalize.css, MIT](https://github.com/necolas/normalize.css/blob/5.0.0/LICENSE.md), by Nicolas Gallagher and Jonathan Neal. |
| Noise mask and abstract light backdrop | `noise-base.png` and `abstract-light.jpg` are served from CodePen asset account 13471. The asset URLs identify the hosting account. Image authorship and independent redistribution terms are unverified. |
| Fonts | The Pen and template import Bunny Fonts stylesheets. Amaranth, Asap and Asap Condensed WOFF2 files actually loaded during capture are pinned. The stylesheets also declare Kode Mono and M PLUS 1 Code. Check the family license and retain required notices when redistributing. [Bunny Fonts licensing FAQ](https://fonts.bunny.net/faq) |
| SVG | The Pen supplies inline paths for search, calendar, calculator, messages, profile and billing, plus Twitter and GitHub marks in its attribution links. The Pen supplies the path data without a separate icon-set credit. Brand marks retain their respective ownership. |

## Reproduce the frames

From the repository root on Windows with Node.js 24 and network access, run:

```powershell
$browser = Join-Path $env:TEMP 'css-gpui-neon-browser'
$frames = Join-Path $env:TEMP 'css-gpui-neon-frames'
npm.cmd install --prefix $browser --no-audit --no-fund --save-exact playwright@1.60.0
node "$browser/node_modules/playwright/cli.js" install chromium
node fixtures/neon-glass/capture.mjs $browser $frames
```

The capture verifies the Pen, the shared stylesheet render, and each requested asset before use. It saves 12 full-viewport PNGs, a frame manifest, the measured menu geometry, and the linked CSS to the output directory. On the recorded Windows configuration, open and closed frames use exact PNG hashes. Hover frames also allow a three-level RGB channel tolerance on a 20 × 20 pixel sample grid. Repeated captures showed differences of up to two levels per channel in hover frames. Other platforms can replay the interactions and check input hashes, Chromium version and placement.

The harness sets device scale to 1 and seeds both saved hues to the CSS defaults of 255 and 222. The original JavaScript otherwise chooses random hues on a fresh load. Each case waits for fonts and animations to settle. Filenames are `<width>-<position>-<state>.png`.

| Viewport | Upper-left click | Lower-right click |
| --- | --- | --- |
| 1100 × 760 | (80, 120) | (1092, 752) |
| 390 × 844 | (16, 120) | (382, 836) |

At each width the two clicks place the menu over different areas of the same centered backdrop. The lower-right case exercises the Pen's edge clamp. The replay opens via right-click, hovers Calculator, and closes with an outside click. It verifies the expected menu position, hover state, and visibility changes before recording the respective frames.

## Compare GPUI geometry

With the browser replay output still available, run from the repository root:

```powershell
$env:NEON_GLASS_REFERENCE = $frames
cargo test --locked --manifest-path fixtures/neon-glass/native/Cargo.toml
```

The test compiles the pinned Pen and its linked CSS through Scene IR, checks deterministic Rust emission, and measures the generated GPUI elements in offscreen windows. It compares the panel, inner column, search container and input, first and last rows, and divider against Chromium within one logical pixel at each viewport.

To emit the Rust with diagnostics for features beyond the current renderer:

```powershell
cargo run --example generate_gpui -- --allow-loss --viewport 390x844 fixtures/neon-glass/source.html (Join-Path $frames 'linked.css')
```

## Compare the glass and glow

Run the capture with `--effects` to record both placements, the stage without UI text, and the verified assets. Then run the native compositor comparison and its GPUI gesture test:

```powershell
node fixtures/neon-glass/capture.mjs $browser $frames --effects
$env:NEON_GLASS_REFERENCE = $frames
cargo run --locked --manifest-path fixtures/neon-glass/native/Cargo.toml --example effects
cargo test --locked --manifest-path fixtures/neon-glass/native/Cargo.toml --all-targets -- --include-ignored
```

The comparison isolates paint by using the same panel bounds in both renderers. It writes the Chromium PNGs, the raster textures painted by GPUI, amplified RGB difference images, and error metrics to the capture directory. The compositor covers the authored image and hard-light gradient, sampled blur, panel gradients and border, outer shadows, and conic and luminance-masked glow layers. Its Gaussian blur uses three sliding box passes. The comparison measures channels in sRGB on a 0–255 scale.

| Comparison region | Original backdrop | Backdrop shifted 90 px |
| --- | ---: | ---: |
| Stage mean RGB error | 0.734 | 0.856 |
| Glass interior mean RGB error | 0.487 | 0.349 |
| Panel and glow mean RGB error | 1.585 | 1.470 |
| Panel and glow largest channel error | 98 | 102 |

The remaining differences concentrate at the panel edges and around the brightest glow. The compositor approximates corner antialiasing, noise resampling, shadow kernels, and OkLCh interpolation. The test isolates the static settled effects from animated opening and hover styles. The color-space conversion follows the OKLab matrices in [CSS Color 4](https://www.w3.org/TR/css-color-4/#color-conversion-code).

Add `-- --window` to the `cargo run` command to preview the textures in GPUI. Click in the preview to move the backdrop under the stationary panel. The gesture rerenders from the underlying asset and recalculates the blurred sample.

The standalone blur probe uses synthetic color bands and compares the interior against Chromium at two offsets:

```powershell
$env:NEON_BACKDROP_REFERENCE = Join-Path $env:TEMP 'css-gpui-blur-probe'
node fixtures/neon-glass/backdrop-reference.mjs $browser $env:NEON_BACKDROP_REFERENCE
cargo test --locked --test backdrop -- --include-ignored --nocapture
```

## Run the standalone native menu

### Build the local menu on Linux

Install Python 3, Node.js 24 with npm, and the [Rust 1.95.0, X11 build libraries, and Vulkan runtime used by the toolbar](../../docs/first-milestone.md#build-and-run-the-toolbar). From the repository root, set up the pinned browser tools once. Install the Chromium system libraries for your distribution if Playwright requests them.

```sh
browser="${TMPDIR:-/tmp}/css-gpui-neon-browser"
npm install --prefix "$browser" --no-audit --no-fund --save-exact playwright@1.60.0 wawoff2@2.0.1
node "$browser/node_modules/playwright/cli.js" install chromium
```

Edit the CSS inside [source.html](source.html), for example the text color in the `#menu` rule. Run Build from the repository root.

```sh
python3 fixtures/neon-glass/build.py "$browser"
```

Build measures the edited source with Chromium, verifies each fetched fixture dependency against [reference.json](reference.json), and compiles the existing GPUI menu. It writes the stage and overlay Rust for both viewport sizes under `target/neon-glass/generated/`. Run `target/neon-glass/neon-glass` on X11 or XWayland. The executable embeds its assets and runs without Chromium, model credentials, or the temporary capture directory. Build fetches dependencies over the network. Keep the executable and its embedded third-party assets for local use under the provenance terms above.

The `--build` capture mode accepts local source edits and measures geometry. The standard reference capture commands still check the pinned source and replay the frozen screenshot protocol.

### Run the reference build on Windows

Prepare the verified assets and OpenType fonts, then build and run:

```powershell
npm.cmd install --prefix $browser --no-audit --no-fund --save-exact wawoff2@2.0.1
node fixtures/neon-glass/capture.mjs $browser $frames --native
$env:NEON_GLASS_REFERENCE = $frames
cargo run --locked --manifest-path fixtures/neon-glass/native/Cargo.toml --features interactive --example menu
```

The build embeds the authored image, noise mask, compiled scene and the pinned font subsets. The capture converts the verified WOFF2 bytes to OpenType using [wawoff2](https://github.com/fontello/wawoff2). The resulting executable runs with Chromium closed and with no capture directory or environment variable at runtime. Pass `-- 90` to start with the shifted backdrop. Right-click outside the menu to move or open it, move over a row to highlight it, and left-click outside to close it.

The native replay checks the generated Calculator bounds, viewport clamping, row hover and selection, the 200 ms selection reset, the 250 ms opacity transition, and the 500 ms close visibility delay. It exercises the two comparison backdrops and the narrow window. On Windows, the stage uses normal-width Asap because GPUI's DirectWrite font lookup requests normal stretch. The menu itself uses the pinned Asap face. The search field is a static preview. The demo reproduces the pointer sequence and settled effects. It approximates the source's flash and filter animation and omits live hue sliders and editing.

```powershell
cargo test --locked --manifest-path fixtures/neon-glass/native/Cargo.toml --features interactive --all-targets -- --include-ignored
```

### Compare glass in the standalone Windows client

After the native browser capture above, build the menu example and capture its actual client area on the original and +90 px backdrops. Use an interactive Windows desktop session.

```powershell
python -m pip install windows-capture==2.0.1 Pillow==10.4.0
$manifest = 'fixtures/neon-glass/native/Cargo.toml'
cargo build --locked --manifest-path $manifest --features interactive --example menu
$target = (cargo metadata --no-deps --format-version 1 --manifest-path $manifest | ConvertFrom-Json).target_directory
$menu = Join-Path $target 'debug/examples/menu.exe'
foreach ($shift in @(0, 90)) {
    python fixtures/neon-glass/capture_windows.py $menu "$frames/gpui-startup-1100-shift$shift.png" 1100 760 $shift
}
python fixtures/neon-glass/compare_startup.py $frames
```

The checker normalizes the client PNG to the Chromium logical viewport and reports mean RGB errors for the whole panel, the glass over the bright backdrop line, and each glow corner. To check detection, temporarily omit the glass canvas in `MenuView::render`, rebuild, and capture both GPUI PNGs again. The comparison should exit with status 1.

## Input inventory

- HTML uses a stage with header and hue controls and a fixed menu aside. The menu has six decorative shine and glow spans, a search label and text input, and two sections of focusable list rows separated by a rule.
- Menu CSS uses nested selectors, custom properties and `@property`, fixed positioning, grid and flex layout, transparent gradients in OKLab and OKLCH, rounded borders, shadow, backdrop blur, masked pseudo-elements, image noise, transitions and glow and flash keyframes. Hover, focus and selected styles apply to menu rows. The 480 px media query changes the footer gap.
- The stage uses a centered cover image blended with a hue gradient. The shared stylesheet provides page defaults, color variables, typography and form control styling.

## Host event bindings

| Browser event | Behavior |
| --- | --- |
| Document `contextmenu` | Suppress the browser context menu. For a target outside the custom menu, position it at the cursor, clamp against the right and bottom edges with 30 px and 20 px margins, add `open`, and cancel the pending reset. |
| Document `pointerdown` | Outside the menu and any input, remove `open` and clear the search field and row selection after 200 ms. Inside, clear row selections and select the target if the direct target is an `li`. |
| Hue slider `input` | On the next animation frame, update the matching body custom property, persist the value, and open the menu. |
| Randomize button `click` | Pick a new pair of hues, update the sliders and body properties, and open the menu. |
| Initialization | Read both saved hues from local storage with random defaults. The HTML initially marks the menu as open and Calendar as selected. |

The search input accepts text and the rows expose hover, focus and selection. The Pen defines no command dispatch, search filtering, or Escape handler.

## MIT notice for the Pen code

Copyright (c) 2025 Simey (simeydotme)

Permission is hereby granted, free of charge, to any person obtaining a copy of
this software and associated documentation files (the "Software"), to deal in
the Software without restriction, including without limitation the rights to
use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies
of the Software, and to permit persons to whom the Software is furnished to
do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
