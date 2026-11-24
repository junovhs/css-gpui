# First milestone · a resizable toolbar

Ship one end-to-end conversion a GPUI developer can inspect in a minute. The fixture has a title on the left and a labeled button on the right. The author sets a flex row, center alignment, horizontal padding, and `justify-content: space-between`. The parent supplies the available width. Keep the fixture beside the generated example so both renderers can be checked from a clean checkout.

## Build order

1. Create the toolbar HTML and CSS. Set button margin, padding, border, and font explicitly. Capture the approved browser appearance and bounds.
2. Carry `space-between` through the frontend and reference emitter. Exercise it alongside a negative fixture containing a deliberately unsupported declaration.
3. Emit a GPUI component with the correct child order, text, styling, and flex rule. Compile it inside a minimal host against the pinned GPUI revision. Leave the application action to its caller.
4. Run the same cases in the browser and native host. Record the source, generated Rust, per-element bounds, and both images with the comparison results.

## Acceptance cases

| Case | Evidence required |
| --- | --- |
| Short title at 360 and 720 logical pixels | At both widths the title begins at the leading inset and the action ends at the trailing inset. Both children remain vertically centered. |
| Longer title at both widths | Swap “Files” for “Recently opened files”. The label and button remain legible and keep the browser's spacing and alignment. |
| Source inspection | The Rust expresses the flex relationship through GPUI layout. There are no per-viewport coordinates. Running conversion twice produces identical Rust. |
| Unsupported declaration | The tool identifies the source declaration and explains the loss. The example cannot be recorded as a clean conversion. |

For the first bounds checks, use a proposed tolerance of one logical pixel per edge with a fixed font and scale. Review text rasterization differences separately and document any tolerance change with a failing case. A successful milestone includes the compiling Rust and the two-width comparisons. The imported fixtures alone establish the frontend baseline.
