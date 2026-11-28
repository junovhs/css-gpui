#!/bin/sh
set -eu

root=$(CDPATH='' cd "$(dirname "$0")/.." && pwd)
browser="$root/target/release-browser"

if [ ! -f "$browser/node_modules/playwright/package.json" ]; then
    echo 'Run sh release/setup.sh to install the pinned build tools first.' >&2
    exit 1
fi

python3 "$root/fixtures/neon-glass/build.py" "$browser"
python3 "$root/fixtures/toolbar/build.py"
