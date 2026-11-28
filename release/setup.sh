#!/bin/sh
set -eu

release=$(CDPATH='' cd "$(dirname "$0")" && pwd)
browser="$release/../target/release-browser"

rustup toolchain install 1.95.0 --profile minimal
mkdir -p "$browser"
cp "$release/package.json" "$release/package-lock.json" "$browser"
npm ci --prefix "$browser" --no-audit --no-fund
node "$browser/node_modules/playwright/cli.js" install chromium
