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
