# First milestone · a resizable toolbar

Ship one end-to-end conversion a GPUI developer can inspect in a minute. The fixture has a title on the left and a labeled button on the right. The author sets a flex row, center alignment, horizontal padding, and `justify-content: space-between`. The parent supplies the available width. Keep the fixture beside the generated example so both renderers can be checked from a clean checkout.

## Build and run the toolbar

On Linux with X11 or XWayland, install Python 3 and Rust 1.95.0 through rustup (`rustup toolchain install 1.95.0`). The native host needs a C/C++ toolchain, pkg-config, and the XCB, xkbcommon (including X11), Fontconfig, and FreeType development libraries. On Debian or Ubuntu, the corresponding packages are `build-essential pkg-config libxcb1-dev libxkbcommon-dev libxkbcommon-x11-dev libfontconfig1-dev libfreetype-dev`. Run the executable with a Vulkan loader and driver installed, such as Debian or Ubuntu's `libvulkan1` and `mesa-vulkan-drivers`.

From the repository root, run Build:

```sh
python3 fixtures/toolbar/build.py
```

Build reads [the original HTML](../fixtures/toolbar/source.html) and [CSS](../fixtures/toolbar/source.css), generates `target/toolbar/generated.rs`, and compiles it into `target/toolbar/debug/css-gpui-toolbar-host` using the locked GPUI host. The [checked-in generated Rust](../fixtures/toolbar/native/src/generated.rs) shows the result for the original design. If an edit uses unsupported CSS, Build reports the diagnostic and exits with an error.

Launch the native window with `./target/toolbar/debug/css-gpui-toolbar-host`. Change `.toolbar`'s `background-color` in the CSS and repeat Build before relaunching to see the change. Generation and launch run locally without a browser or model credentials. Cargo may download dependencies on the first build.

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
