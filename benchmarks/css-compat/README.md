# Independent CSS compatibility benchmark

This benchmark measures direct conversion of a small, fixed set of human and AI screens. It asks whether the unmodified authored HTML and CSS can produce a lossless GPUI component with comparable pixels, viewport layouts and CSS-driven states. Treat these figures as measurements for the named inputs and compiler revision only. [November 27, 2026 results](results/2026-11-27.md)

## Corpus and scoring

[The corpus lock](corpus.json) defines the source bytes, browser dependencies, evaluation split, viewports, visual thresholds, browser probes and interaction scenarios. The selection covers a historical live utility, a recent deployed app, independent Pens, and standalone generated design prompts. The browser sees the same original documents and linked styles, with browser scripting used where the original requires it. The compiler receives original HTML plus linked stylesheet contents concatenated in document order. The runner uses direct source compilation and a generic GPUI host.

| Check | Scoring rule |
| --- | --- |
| Clean conversion | The ordinary `generate_gpui --viewport` CLI exits successfully, emits a renderer and prints no loss or error. Every evaluation screen counts at both viewport sizes. |
| Native build | Rust from a clean conversion must build as a standalone GPUI client. A failed conversion blocks its strict native rows, which stay in the denominator. |
| Baseline appearance | At 1× scale and the same client dimensions, a strict native frame passes when normalized mean absolute RGB error is at most 0.03 and at most 10% of RGB channels differ by more than 16 of 255. The screenshot score can miss small local defects. |
| Reflow | Compare the browser probes and captured layouts at both 390×844 and 1280×900. Both strict native layouts must be usable before claiming matched reflow. These are independent source compilations at specified viewport sizes. Live window resizing is outside this check. |
| CSS-driven interaction | Compare browser and native evidence for each applicable keyboard focus, form, wheel scroll, theme or motion state. Browser probes record the action result. Native interaction stays unverified if a usable strict baseline or paired native action is missing. |

An `--allow-loss` run is diagnostic. The runner retains the emitted Rust and both stages of diagnostics when available. A screenshot of that Rust may show how a screen degrades. It contributes zero strict passes. A feature family denominator counts each evaluation screen tagged with that family once, with both viewports required. A single screen may belong to several families. Classes distinguish a confirmed loss, a native mismatch and a check blocked by an earlier failure.

The score covers the initial window and frozen gestures. Application business logic, analytics, web services, content below the viewport and platform-only affordances are outside the grade. The reference replays locally pinned bytes and blocks other network requests. The TaskFlow favicon and CodePen shell icons sit outside the measured page. CodePen’s injected loop watchdog is skipped. Contrast Ratio’s old GitHub ribbon URL uses GitHub’s current copy of the same ribbon, and its Incrementable dependency is pinned to the last commit before the archived screen. Third-party fonts and the historical dependency are fetched by hash and need no redistribution in this repository. CodePen files are the full-page renderer’s decoded `srcdoc`, with the author’s runnable HTML, CSS and JavaScript retained. They are distributed under CodePen’s public-Pen MIT license. [CodePen licensing](https://blog.codepen.io/documentation/licensing/), [GitHub ribbon archive](https://github.blog/news-insights/github-ribbons/)

## Source provenance

| Screen | Origin and split |
| --- | --- |
| Contrast Ratio | Original published contrast-checking utility. Evaluation uses the 2016 release-era source by Lea Verou. [Pinned source](https://github.com/siege-media/contrast-ratio/tree/97af6d1a356f9c4435576aa4cda3900bd90026f3), [author’s history](https://lea.verou.me/projects/) |
| TaskFlow | Published task-management app with its August 2026 repository source, including the startup screen and theme control. Evaluation. [Pinned source](https://github.com/OM-ALONI/TaskFlow/tree/c43bcfaf0bd19c07a3c99c89afb87a1e1127d97d), [deployed app](https://om-aloni.github.io/TaskFlow/) |
| CSS Scroll Snap Carousel | Independent CSS and JavaScript Pen by `newinweb`. Evaluation includes the authored initial orientation and CSS wheel snapping. [Original Pen](https://codepen.io/newinweb/pen/EpPgdR) |
| Accessible Form Controls | Independent Pen by `joeflash`. Reserved holdout with keyboard focus and CSS checkbox/switch states. [Original Pen](https://codepen.io/joeflash/pen/qBYLbpw) |
| Library Room Booking | Fresh Qwen2.5-Coder 1.5B Instruct via llama.cpp. Evaluation uses the first response, with its enclosing Markdown fence removed. [Exact prompt](prompts/library.txt), [model](https://ollama.com/library/qwen2.5-coder:1.5b-instruct-q4_K_M) |
| Upcoming Movies | Fresh SmolLM2 1.7B Instruct via Ollama. Evaluation uses the first complete self-contained response. [Exact prompt](prompts/cinema-local.txt), [model](https://ollama.com/library/smollm2:1.7b-instruct-q4_K_M) |

The two complete self-contained AI designs came from four generation attempts. The first cinema attempt reached the token limit before closing its HTML and used external styles. The second closed its HTML and still relied on external styles and scripts. Both original responses remain in the corpus as acquisition evidence and were excluded before compiler testing. The successful third cinema request used a shorter design prompt that retained the local-file requirement. All prompts, tool versions, settings and model file hashes are in the lock. Neither tool saw compiler documentation, test results or source code. The brands, workflows and visual quality choices come from the raw model outputs.

## Reproduce

Run from the repository root with Node.js 22+, Rust, Cargo and a Playwright 1.60.0 installation with its pinned Chromium. `mktemp` keeps the outputs outside the working tree. Chromium writes the baseline PNG, scenario PNGs, computed styles and bounds, and any replay errors. Strict and exploratory compiler logs are kept per viewport. The output folder must be new.

```sh
scratch=$(mktemp -d)
node benchmarks/css-compat/fetch-assets.mjs "$scratch/assets"
node benchmarks/css-compat/run.mjs /path/to/playwright-install "$scratch/assets" "$scratch/evaluation"
```

Run the native image pass in an isolated Linux X11 session with `xvfb-run`, `xdotool`, ImageMagick's `import`, Pillow, the XKB/X11 development libraries and Vulkan. Mesa lavapipe can present to Xvfb when the hardware Vulkan driver requires DRI3. Use the lavapipe ICD installed on the machine. The native pass records build and window logs, exact client PNGs, RGB differences and the comparison result in `native-results.json`. It replays available strict gestures and records pixel changes before and after each action. The host leaves native theme settings and default paint as supplied by GPUI.

```sh
VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json xvfb-run -a \
  --server-args='-screen 0 1600x1200x24' \
  python3 benchmarks/css-compat/capture-native.py "$scratch/evaluation"
```

## Editable text input probe

The standalone probe uses the pinned Chromium and the ordinary GPUI CLI and host. At both benchmark widths it verifies that Tab focuses the first field, typed text changes the visible field, and Tab and Shift+Tab move between fields without discarding their values. The native check reads each focused field's value through the X11 clipboard. The probe also saves browser and native frames.

Use the same Xvfb and Vulkan setup as the native pass, with `xclip` and ImageMagick's `identify` and `convert` commands available.

```sh
scratch=$(mktemp -d)
VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json xvfb-run -a \
  --server-args='-screen 0 1600x1200x24' \
  node benchmarks/css-compat/text-input-probe.mjs /path/to/playwright-install "$scratch/text-input"
```

## Flex wrapping probe

The standalone probe compiles the same authored flex row for 390×844 and 1280×900. It compares Chromium DOM bounds with pixel-measured bounds from the generated GPUI window. The third child starts a new line at 390 px, and all three fit on the first line at 1280 px. The probe saves both frames and the measured bounds for each viewport.

Use the same Playwright, Xvfb and Vulkan setup as the benchmark.

```sh
scratch=$(mktemp -d)
VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json xvfb-run -a \
  --server-args='-screen 0 1600x1200x24' \
  node benchmarks/css-compat/flex-wrap-probe.mjs /path/to/playwright-install "$scratch/flex-wrap"
```

## Text alignment probe

The probe renders 216 px blocks with start, inherited center and end alignment, plus a centered editable field. It compares the left and right edges of the text ink in Chromium and GPUI screenshots. The tolerance is 2 logical px per edge to account for font rasterization and subpixel rounding at 1× scale. The probe saves both frames and the positions. The compiler preserves left and right as physical CSS keywords and maps start and end using left-to-right flow. Bidirectional alignment, justification and wrapped-line parity are outside this probe.

Use the same Playwright, Xvfb and Vulkan setup as the benchmark, with DejaVu Sans Mono installed.

```sh
scratch=$(mktemp -d)
VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json xvfb-run -a \
  --server-args='-screen 0 1600x1200x24' \
  node benchmarks/css-compat/text-align-probe.mjs /path/to/playwright-install "$scratch/text-align"
```

## Letter spacing probe

The probe compares the advance of a DejaVu Sans Mono label with positive and negative letter spacing against an unspaced control. Chromium reports the label width through DOM geometry. In the GPUI window, the next flex sibling marks the end of that advance. The probe also compares the change in the right edge of the glyph ink to verify painting. Both changes must agree within 1 logical px at 1× scale to account for rasterization and layout rounding. The output contains both screenshots and the measured advances.

The compiler resolves signed `px`, `em` and `rem` lengths, unitless zero, `normal`, `initial`, `inherit` and `unset`. HTML replay writes the computed spacing. GPUI applies nonzero spacing to single-word labels composed of ASCII letters or digits. Other text and editable inputs report a letter-spacing target loss.

Use the same Playwright, Xvfb and Vulkan setup as the benchmark, with DejaVu Sans Mono installed.

```sh
scratch=$(mktemp -d)
VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json xvfb-run -a \
  --server-args='-screen 0 1600x1200x24' \
  node benchmarks/css-compat/letter-spacing-probe.mjs /path/to/playwright-install "$scratch/letter-spacing"
```

## Hover and focus paint probe

The probe drives pointer entry and exit and Tab and Shift+Tab in Chromium and the ordinary generated GPUI window. It compares the first control's fill, border, and label colors in all ten captures, including states where both selectors match and one property returns to its resting value. The second control's fill confirms Tab transfers focus between the button and an editable input.

The compiler supports `:hover`, `:focus`, and their conjunction as suffixes on the selector for the element being painted. Hover uses generated boxes and controls. Focus uses enabled buttons and editable single-line inputs in default tab order. Supported changes are solid background and existing border colors, and text color on elements without nested elements. The CSS color parser and static custom properties apply. Other interactive selectors, layout changes, outlines, shadows, layered backgrounds, and custom tab order produce source losses.

Use the same Playwright, Xvfb and Vulkan setup as the benchmark.

```sh
scratch=$(mktemp -d)
VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json xvfb-run -a \
  --server-args='-screen 0 1600x1200x24' \
  node benchmarks/css-compat/state-paint-probe.mjs /path/to/playwright-install "$scratch/state-paint"
```

## Holdout and refresh

The holdout is frozen with the evaluation sources. Keep it out of routine runs and out of compiler tuning. Run the same browser and native commands on that split with `--holdout` passed to `run.mjs` when evaluating a later candidate. Record holdout results separately and retire each disclosed holdout from future gate decisions.

For each breadth re-evaluation, sample fresh output from at least two independent AI tools using ordinary application design prompts. Save the exact prompts, raw outputs, tool and model identifiers, and any transport-level source extraction. Keep the first complete result, including designs that use CSS the compiler cannot consume. Archive old corpus versions and freeze the updated bytes, feature tags, interactions, thresholds and split before running the compiler. Refresh the holdout independently. Leave fixes driven by evaluation failures out of the frozen source and log any exclusion with its scope.
