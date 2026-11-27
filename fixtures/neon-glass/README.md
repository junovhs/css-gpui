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

The capture verifies the Pen, the shared stylesheet render, and each requested asset before use. It saves 12 full-viewport PNGs, a frame manifest and the extracted shared stylesheet to the output directory. On the recorded Windows configuration, ten frames use exact PNG hashes. The two narrow hover frames also allow a three-level RGB channel tolerance on a 20 x 20 pixel sample grid. Repeated captures showed differences of up to two levels per channel in those two frames. Other platforms can replay the interactions and check input hashes, Chromium version and placement.

The harness sets device scale to 1 and seeds both saved hues to the CSS defaults of 255 and 222. The original JavaScript otherwise chooses random hues on a fresh load. Each case waits for fonts and animations to settle. Filenames are `<width>-<position>-<state>.png`.

| Viewport | Upper-left click | Lower-right click |
| --- | --- | --- |
| 1100 × 760 | (80, 120) | (1092, 752) |
| 390 × 844 | (16, 120) | (382, 836) |

At each width the two clicks place the menu over different areas of the same centered backdrop. The lower-right case exercises the Pen's edge clamp. The replay opens via right-click, hovers Calculator, and closes with an outside click. It verifies the expected menu position, hover state, and visibility changes before recording the respective frames.

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
