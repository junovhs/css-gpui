#!/bin/sh
set -eu

if [ "$#" -gt 1 ]; then
    echo 'Usage: sh release/package.sh [commit] > css-gpui-linux-rc.tar.gz' >&2
    exit 2
fi

root=$(CDPATH='' cd "$(dirname "$0")/.." && pwd)
commit=$(git -C "$root" rev-parse --verify --end-of-options "${1:-HEAD}^{commit}")

# Select build inputs and provenance from the same commit. The Neon Glass
# external image and font bytes are fetched and verified by Build.
git -C "$root" archive --format=tar.gz --prefix=css-gpui-linux-rc/ "$commit" -- \
    Cargo.toml Cargo.lock src examples/generate_gpui.rs release \
    fixtures/neon-glass/README.md fixtures/neon-glass/source.html \
    fixtures/neon-glass/reference.json fixtures/neon-glass/build.py \
    fixtures/neon-glass/capture.mjs fixtures/neon-glass/native \
    fixtures/toolbar/build.py fixtures/toolbar/source.html \
    fixtures/toolbar/source.css fixtures/toolbar/native
