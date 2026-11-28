#!/usr/bin/env python3
"""Prepare and run the Backrooms source preview on Linux."""

import argparse
import os
from pathlib import Path
import re
import subprocess
from tempfile import TemporaryDirectory

from run import DOCUMENT_SHA256, REPOSITORY, digest, download_source, verify_source


OUTPUT = REPOSITORY / "target" / "backrooms"
SOURCE = OUTPUT / "input.html"
BIN = "css-gpui-planar-preview"
EXECUTABLE = OUTPUT / "release" / BIN
MANIFEST = REPOSITORY / "benchmarks" / "css-compat" / "native" / "Cargo.toml"
DEFAULT_VIEWPORT = "1152x648"


def viewport_size(value):
    size = re.fullmatch(r"([0-9]{1,4})x([0-9]{1,4})", value)
    if size is None or not all(1 <= int(length) <= 4096 for length in size.groups()):
        raise argparse.ArgumentTypeError("expected WIDTHxHEIGHT with both dimensions in 1..4096")
    return value


def prepare():
    if not SOURCE.is_file() or digest(SOURCE.read_bytes()) != DOCUMENT_SHA256:
        document = download_source()
        verify_source(document)
        OUTPUT.mkdir(parents=True, exist_ok=True)
        with TemporaryDirectory(dir=OUTPUT) as directory:
            staged = Path(directory) / SOURCE.name
            staged.write_bytes(document)
            staged.replace(SOURCE)

    subprocess.run(
        ["cargo", "+1.95.0", "build", "--locked", "--release", "--manifest-path", MANIFEST,
         "--features", "planar-preview", "--bin", BIN, "--target-dir", OUTPUT],
        cwd=REPOSITORY,
        check=True,
    )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument("--prepare-only", action="store_true", help="fetch and build without opening a window")
    mode.add_argument("--offline", action="store_true", help="launch the prepared files without curl or Cargo")
    parser.add_argument("--viewport", type=viewport_size, default=DEFAULT_VIEWPORT,
                        help=f"scene size in pixels (default: {DEFAULT_VIEWPORT})")
    args = parser.parse_args()

    if not args.offline:
        prepare()
    if not SOURCE.is_file() or not EXECUTABLE.is_file():
        raise SystemExit("Run python3 benchmarks/backrooms/play.py --prepare-only first.")
    verify_source(SOURCE.read_bytes())
    if args.prepare_only:
        print(f"Pinned source: {SOURCE}\nNative executable: {EXECUTABLE}")
        return

    # The original document contains its stylesheet inline.
    os.execv(EXECUTABLE, [str(EXECUTABLE), "--document", args.viewport,
                         str(SOURCE), os.devnull, "content", "body"])


if __name__ == "__main__":
    try:
        main()
    except subprocess.CalledProcessError as error:
        raise SystemExit(error.returncode)
    except OSError as error:
        raise SystemExit(str(error))
