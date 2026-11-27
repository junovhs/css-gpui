"""Check the glass and glow in matched standalone GPUI and Chromium client PNGs."""

import argparse
import json
from pathlib import Path

from PIL import Image, ImageChops, ImageStat


def compare(frames, native):
    geometry = json.loads((frames / "geometry.json").read_text(encoding="utf-8"))
    wide = next(case for case in geometry if case["width"] == 1100)
    viewport = (wide["width"], wide["height"])
    x, y, width, height = map(round, wide["boxes"]["menu"])
    right, bottom = x + width, y + height

    # The right interior strip crosses the light beam and avoids the controls.
    # The corner regions include the border and the glow beyond the menu.
    regions = {
        "wholePanel": ((x, y, right, bottom), 6),
        "glassAcrossBackdrop": ((right - 14, y + 180, right - 3, y + 276), 3),
        "purpleGlow": ((right - 45, y - 18, right + 22, y + 42), 14),
        "blueGlow": ((x - 18, bottom - 42, x + 52, bottom + 19), 18),
    }
    results = {}
    for shift in (0, 90):
        suffix = f"startup-1100-shift{shift}.png"
        with Image.open(frames / f"chromium-{suffix}") as browser, Image.open(
            native / f"gpui-{suffix}"
        ) as window:
            if browser.size != viewport:
                raise ValueError(f"Chromium {suffix} has size {browser.size}, expected {viewport}")
            expected_height = window.width * viewport[1] / viewport[0]
            if abs(window.height - expected_height) > 1:
                raise ValueError(f"GPUI {suffix} has the wrong client aspect ratio: {window.size}")
            logical = window.convert("RGB").resize(viewport, Image.Resampling.BOX)
            delta = ImageChops.difference(browser.convert("RGB"), logical)

        metrics = {}
        for name, (bounds, limit) in regions.items():
            mean = sum(ImageStat.Stat(delta.crop(bounds)).mean) / 3
            metrics[name] = {"meanRgbError": round(mean, 3), "limit": limit, "passed": mean <= limit}
        results[f"shift{shift}"] = metrics
    return results


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("frames", type=Path, help="Chromium startup PNGs and geometry.json")
    parser.add_argument("--native", type=Path, help="GPUI startup PNG directory (defaults to frames)")
    args = parser.parse_args()
    results = compare(args.frames, args.native or args.frames)
    print(json.dumps(results, indent=2))
    if any(not metric["passed"] for case in results.values() for metric in case.values()):
        raise SystemExit(1)
