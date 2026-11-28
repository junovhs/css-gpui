# CSS to GPUI on Linux

[Neon Glass Context Menu by Simey (@simeydotme)](https://codepen.io/simeydotme/pen/PwwYZOe) is the showcase in this source release candidate. See [Simey’s profile](https://codepen.io/simeydotme) and the [Pen’s source notice and asset provenance](../fixtures/neon-glass/README.md). The toolbar is a compact example of the strict generator flow.

## Set up once

Use an x86-64 Linux desktop with X11 or XWayland and a working Vulkan driver. Install Python 3, Node.js 24 with npm, rustup, a C/C++ toolchain and pkg-config. On Debian or Ubuntu, the build and display libraries are available with:

```sh
sudo apt install build-essential pkg-config libxcb1-dev libxkbcommon-dev \
  libxkbcommon-x11-dev libfontconfig1-dev libfreetype-dev libvulkan1 mesa-vulkan-drivers
```

From the extracted archive or repository root, install Rust 1.95.0 and the locked Node tools and Chromium:

```sh
sh release/setup.sh
```

If Playwright reports missing Chromium libraries on a supported Debian or Ubuntu system, run `node target/release-browser/node_modules/playwright/cli.js install-deps chromium`.

## Edit, build and run

Change `color: #737985;` in the `#menu` rule in [Neon Glass’s source](../fixtures/neon-glass/source.html) to `color: #ff6bd6;`. Then run Build once:

```sh
sh release/build.sh
```

| Sample | Readable generated Rust | Launch from the package root |
| --- | --- | --- |
| [Neon Glass Context Menu by Simey (@simeydotme)](https://codepen.io/simeydotme/pen/PwwYZOe) | `target/neon-glass/generated/` has stage and overlay modules at 390 and 1100 logical pixels | `./target/neon-glass/neon-glass` |
| [Toolbar](../fixtures/toolbar/source.css) | `target/toolbar/generated.rs` | `./target/toolbar/debug/css-gpui-toolbar-host` |

Build uses Chromium to measure Neon Glass and verifies the external stylesheet, image and font dependencies against [their pinned SHA-256 hashes](../fixtures/neon-glass/reference.json). The native host wires right-click positioning, row hover and selection, and outside-click dismissal. It composites the approximated glass and glow. The search field is a static preview and hue editing is outside the native demo. The toolbar runs the strict generator and reports unsupported CSS as an error at its source location. For a second edit, change the toolbar’s `background-color` from `#151922` to `#254631`.

You can run both executables offline from another directory by passing their absolute paths. They use the local display, system libraries and Vulkan driver. Neon Glass embeds its measured build inputs and assets. Both launches work with Chromium closed and without model credentials or build environment variables.

Build also leaves the compiler at `target/toolbar/compiler/debug/examples/generate_gpui`. To run the strict converter directly:

```sh
target/toolbar/compiler/debug/examples/generate_gpui \
  fixtures/toolbar/source.html fixtures/toolbar/source.css
```

## Recreate this source archive

From a Git checkout, pass the release candidate’s full commit ID. Git archives the committed tree at that revision. The commit ID is also stored in the tar header.

```sh
sh release/package.sh "$(git rev-parse HEAD)" > css-gpui-linux-rc.tar.gz
tar -xzf css-gpui-linux-rc.tar.gz
```

The archive carries the compiler and host sources, Cargo and Node lockfiles, sample sources, and verified fetch recipe. Setup and Build need internet access for dependencies. Preserve the Pen’s embedded links and source notice. Keep the locally built Neon Glass executable and its embedded third-party assets for local use while redistribution rights remain unverified. See the [dependency inventory](../fixtures/neon-glass/README.md#neon-glass-browser-reference).
