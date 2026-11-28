# Carousel – rotate

David DeSandro’s original nine-panel example. The tutorial links to this exact Pen and licenses its example code under MIT. The [license](LICENSE) reproduces the author’s linked notice as plain text. [CodePen](https://codepen.io/desandro/pen/jxwELK) · [Author’s tutorial](https://3dtransforms.desandro.com/carousel) · [Upstream example license](https://desandro.mit-license.org/)

The three source files are contiguous byte ranges from the CodePen Debug export fetched on 2026-11-27 (export SHA-256 `5f82d0ccb7e86d53ec8928aa24c7a5a6a73c5c3ac89c518b478eb76383c546e0`). `source.html` runs from the opening scene tag through the controls’ closing paragraph and its LF, with CodePen’s two-space prefix on the first line excluded. `source.css` and `source.js` begin on the line after the export’s opening style and script tags and end with the LF before the closing tags. The replay checks each extracted file’s SHA-256 and serves the exact CSS and JS as local resources with a minimal HTML shell around the body fragment. [CodePen Debug export](https://cdpn.io/desandro/debug/jxwELK)

## Chromium replay

Run these commands from the repository root with Node.js to use Playwright 1.60.0 and its bundled Chromium.

```sh
pw=$(mktemp -d)
npm install --prefix "$pw" playwright@1.60.0
"$pw/node_modules/.bin/playwright" install chromium
out=$(mktemp -d)
node fixtures/carousel-rotate/capture.mjs "$pw" "$out"
```

The replay uses fresh light-mode Chromium contexts at 360 × 280 and 720 × 280 CSS px, both at device scale 1, with network requests served locally. The wide frame shows the full ring projection. The narrow frame clips the outer panels at the viewport edges while keeping both buttons visible. Each click uses the original button, and capture waits for the authored transition to settle.

| Frame suffix | Action from previous frame | JS index | Parent Y rotation | Front panel |
| --- | --- | ---: | ---: | ---: |
| `initial` | Load | 0 | Initial CSS Z translation only | 1 |
| `next` | Click Next | 1 | −40° | 2 |
| `previous` | Click Previous | 0 | 0° | 1 |
| `wrap-previous` | Click Previous at panel 1 | −1 | +40° | 9 |

The [frozen Chromium output](reference/) contains full-viewport PNGs and [browser-reference.json](reference/browser-reference.json). Each row records the PNG digest, initiating button, JS index, inline and computed carousel transforms, scene and button boxes, the front numeral’s text-layout bounds and actual font, and all nine projected cell border quads.

Rectangles are `[x, y, width, height]` in viewport CSS pixels from the top left. Chromium’s border quads are four `x, y` pairs in local box corner order (top-left, top-right, bottom-right, bottom-left before projection), rounded to 0.001 CSS px. Quads include off-screen geometry and retain corner order on back faces. Use them for projection and the PNGs for visible coverage, depth, alpha and glyph paint. The text bounds measure the text range, which includes the font’s line metrics beyond the painted glyph. [Chromium DevTools Protocol](https://chromedevtools.github.io/devtools-protocol/tot/DOM/#method-getBoxModel)

The frozen output was recorded with Chromium 148.0.7778.96 on Linux x64. Two local replays produced identical PNGs and JSON, and all eight PNGs matched a direct replay of the pinned Debug export. On the recording host, compare a replay with `diff -qr fixtures/carousel-rotate/reference "$out"`. System fonts, browser control themes and rasterization can vary with the host. The reference captures settled endpoints. Mid-transition paint and keyboard focus visuals are outside its coverage.

## Behavior for a native port

- **Layout and defaults.** The universal border-box rule makes the fixed scene 210 × 140 px including its 1 px border. The carousel’s 100% absolute width and height resolve to the scene’s 208 × 138 px interior. Its nine absolute 190 × 120 px panels start 10 px from the carousel’s left and top. The scene’s auto side margins center it at both widths, and its 40 px block margins and the browser paragraph layout place the buttons at y = 220 in these captures. Chromium contributes an 8 px body margin, a 16 px default paragraph margin, a white screenshot canvas and native button appearance. The source sets the paragraph’s text alignment to center.
- **Projection and motion.** The scene uses `perspective: 1000px` with the default centered perspective origin (105, 70). The carousel preserves 3D, starts at `translateZ(-288px)`, and transitions its transform for one second with default `ease` timing. Each child applies the authored `rotateY(...) translateZ(288px)` transform in 40° increments with origin (95, 60). The carousel origin is (104, 69). The 1 px horizontal difference between the panel center and the parent origin gives the front quads small subpixel shifts after a turn. The scene’s visible overflow allows projected panels to extend beyond its border.
- **Depth, alpha and text.** The panel fill alone has 0.8 alpha. Each plane carries the fill, opaque 2 px black border and opaque white number. Composite the planes in 3D depth order so rear fills, borders and numerals show through nearer fills. The browser’s default visible back face exposes reversed rear text through the front red panel. The text is centered with an 80 px bold sans-serif font and 116 px line height. Chromium used Liberation Sans Bold for the numerals in this recording. [Wide initial frame](reference/720-initial.png)
- **Host event boundary.** The original JS attaches DOM `click` listeners to the two HTML buttons. Next increments `selectedIndex` by one and Previous decrements it by one. Each writes `translateZ(-288px) rotateY(angle)` to the carousel’s inline transform, where `angle = selectedIndex / 9 * -360` degrees. The native host needs to preserve button activation and those signed index steps. The script has no modulo or reset, so Previous at index 0 sets index −1 and turns +40° to panel 9. Keeping the angle continuous preserves one-step motion across the seam. CSS owns projection, paint and interpolation.

## Standalone GPUI replay

On Linux with X11, Vulkan, the XKB/X11 development libraries, Fontconfig and Liberation Sans installed, build and run from the repository root with Rust 1.95.

```sh
scratch=$(mktemp -d)
manifest=fixtures/carousel-rotate/native/Cargo.toml
cargo +1.95.0 build --locked --manifest-path "$manifest" --target-dir "$scratch/build"
exe="$scratch/build/debug/css-gpui-carousel"
"$exe" 720 280
```

The build reads the unchanged HTML and CSS through the compiler's source graph and cascade. A fixture-specific typed projection carries the scene dimensions, ordered panels, positions, transforms, perspective, timing, colors, label text and typography into the executable. It embeds that data at build time. The GPUI application runs without Chromium, source files or captured geometry. The native button handlers bind the frozen script's signed click steps and extract its turn formula and inline depth.

The viewport uses the same CSS transform-chain projection and inverse plane sampling as the Backrooms renderer. Each panel projects around its CSS origin, clips to the viewport, and is sampled in its local border-box coordinates. It composites panel fills, borders and glyphs rear-to-front in a CPU rasterizer and presents the result as a GPUI texture. The host uses GPUI controls with mouse activation and Tab, Enter and Space support. The signed angle animates using the CSS duration and default ease curve.

To replay both widths, install Xvfb, xdotool, ImageMagick, NumPy and Pillow. Use the Chromium replay instructions above to populate `$out`, or use the checked-in reference directory. Set the Vulkan ICD path to the lavapipe file on your machine for headless capture.

```sh
export VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json
fixture=fixtures/carousel-rotate
xvfb-run -a -s '-screen 0 1280x800x24' "$fixture/native/capture.sh" "$exe" "$scratch/native"
sed 's/hsla(  0,/hsla(180,/' "$fixture/source.css" > "$scratch/edited.css"
CAROUSEL_CSS="$scratch/edited.css" cargo +1.95.0 build --locked \
  --manifest-path "$manifest" --target-dir "$scratch/build"
xvfb-run -a -s '-screen 0 1280x800x24' "$fixture/native/capture.sh" \
  "$exe" "$scratch/edited" --initial-only
browser=${out:-"$fixture/reference"}
python3 "$fixture/native/compare.py" "$browser" "$scratch/native" --edited "$scratch/edited"
```

The capture clicks the GPUI buttons through X11 and compares the resulting window pixels and the quads used by the renderer. Across the eight settled frames, the maximum corner-coordinate difference was 0.00061 CSS px against a 0.01 px tolerance. The comparison measured the following over the full viewport width in the scene's vertical band, with glyphs measured within the browser's front text bounds.

| Check | Acceptance | Observed at both widths |
| --- | ---: | ---: |
| Colored coverage intersection over union | ≥ 0.97 | 0.991–0.998 |
| Near-black stroke intersection over union | ≥ 0.80 | 0.841–0.950 |
| Mean absolute RGB error over visible pixels | ≤ 4.0 / 255 | 1.48–2.46 / 255 |
| Near-white front glyph intersection over union | ≥ 0.87 | 0.898–0.997 |

The hue-only edit repaints the same flat window region from RGB (255, 51, 51) to (51, 255, 255). A mid-turn native capture differs visibly from both endpoints. Chromium comparison covers settled states. The proof covers the frozen ring of disjoint planes, its default origins and back faces, simple numeral rasterization and approximate browser-default controls. It leaves general CSS 3D layouts, intersecting planes, near-plane clipping, browser glyph antialiasing, exact control theming and interrupted-transition parity outside its comparison.
