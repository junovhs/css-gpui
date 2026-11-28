#!/usr/bin/env python3
"""Compare a Chromium replay with real X11 GPUI window captures."""

import argparse
import json
from pathlib import Path

import numpy as np
from PIL import Image


def pixels(file):
    with Image.open(file) as image:
        return np.asarray(image.convert("RGB"), dtype=np.int16)


def intersection_over_union(first, second):
    union = np.count_nonzero(first | second)
    return np.count_nonzero(first & second) / union if union else 0.0


def compare(browser, native):
    frames = json.loads((browser / "browser-reference.json").read_text())["frames"]
    for width in (360, 720):
        expected = [frame for frame in frames if frame["viewport"]["width"] == width]
        actual = [
            json.loads(line)
            for line in (native / f"{width}.jsonl").read_text().splitlines()
        ]
        assert len(expected) == len(actual) == 4
        for reference, projection in zip(expected, actual):
            name = reference["file"]
            assert (projection["width"], projection["height"]) == (width, 280), name
            assert projection["index"] == reference["index"], name
            assert projection["angle"] == (reference["angle"] or 0), name
            corners = np.array(projection["cells"])
            chromium_corners = np.array([cell["borderQuad"] for cell in reference["cells"]])
            assert corners.shape == chromium_corners.shape == (9, 8), name
            error = np.max(np.abs(corners - chromium_corners))
            assert error <= 0.01, (name, "maximum corner error in CSS pixels", error)

            chromium = pixels(browser / name)
            gpui = pixels(native / name)
            assert chromium.shape == gpui.shape == (280, width, 3), name
            _, y, _, height = reference["scene"]
            # The panels extend beyond the scene horizontally. Exclude the
            # control row because the original delegates its style to the UA.
            scene = slice(int(y), int(y + height))
            chromium_scene, gpui_scene = chromium[scene], gpui[scene]
            chromatic = np.ptp(chromium_scene, axis=2) > 20
            native_chromatic = np.ptp(gpui_scene, axis=2) > 20
            black = np.max(chromium_scene, axis=2) <= 25
            native_black = np.max(gpui_scene, axis=2) <= 25
            visible = (np.min(chromium_scene, axis=2) < 248) | (np.min(gpui_scene, axis=2) < 248)
            chromatic_iou = intersection_over_union(chromatic, native_chromatic)
            black_iou = intersection_over_union(black, native_black)
            rgb_error = np.abs(chromium_scene - gpui_scene)[visible].mean()
            assert chromatic_iou >= 0.97, (name, "colored coverage IoU", chromatic_iou)
            assert black_iou >= 0.80, (name, "black stroke IoU", black_iou)
            assert rgb_error <= 4.0, (name, "visible RGB mean absolute error", rgb_error)

            x, y, width_text, height_text = reference["frontTextBounds"]
            text = np.s_[int(y):int(np.ceil(y + height_text)), int(x):int(np.ceil(x + width_text))]
            glyph_iou = intersection_over_union(
                np.min(chromium[text], axis=2) >= 245,
                np.min(gpui[text], axis=2) >= 245,
            )
            assert glyph_iou >= 0.87, (name, "front glyph IoU", glyph_iou)
            print(
                f"{name:23} corners {error:.5f}px | color {chromatic_iou:.3f}"
                f" | stroke {black_iou:.3f} | RGB MAE {rgb_error:.2f} | glyph {glyph_iou:.3f}"
            )

    initial = pixels(native / "720-initial.png")
    turning = pixels(native / "720-turning.png")
    after = pixels(native / "720-next.png")
    for endpoint in (initial, after):
        changes = np.any(np.abs(turning[40:180] - endpoint[40:180]) > 20, axis=2)
        assert np.count_nonzero(changes) >= 2000, "expected a visible intermediate turn"
    return initial


def compare_edit(initial, edited):
    changed = pixels(edited / "720-initial.png")
    assert initial.shape == changed.shape == (280, 720, 3)
    # A flat region inside the leading panel, away from the glyph and
    # neighboring planes. A hue-only edit keeps the authored 0.8 alpha.
    sample = np.s_[57:65, 285:315]
    source_rgb = np.median(initial[sample], axis=(0, 1))
    edited_rgb = np.median(changed[sample], axis=(0, 1))
    assert np.max(np.abs(source_rgb - [255, 51, 51])) <= 1, source_rgb
    assert np.max(np.abs(edited_rgb - [51, 255, 255])) <= 1, edited_rgb
    print(f"CSS hue edit | native leading panel {source_rgb} → {edited_rgb}")


if __name__ == "__main__":
    args = argparse.ArgumentParser(description=__doc__)
    args.add_argument("browser", type=Path)
    args.add_argument("native", type=Path)
    args.add_argument("--edited", type=Path)
    inputs = args.parse_args()
    frame = compare(inputs.browser, inputs.native)
    if inputs.edited:
        compare_edit(frame, inputs.edited)
