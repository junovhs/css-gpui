"""Compare the original Today view and the browser-resolved GPUI render.

With the pinned app running locally:
    python fixtures/noceremony-today/compare_windows.py <noceremony checkout>

Requires Chrome, Pillow, windows-capture==2.0.1, fonttools, brotli, and Rust 1.95.0.
"""

import argparse
from importlib.util import module_from_spec, spec_from_file_location
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

from fontTools.ttLib import TTFont
from PIL import Image, ImageDraw, ImageFont

HERE = Path(__file__).resolve().parent
TOOLBAR = HERE.parent / "toolbar"
spec = spec_from_file_location("toolbar_capture", TOOLBAR / "compare_windows.py")
capture = module_from_spec(spec)
spec.loader.exec_module(capture)


def load_fonts(raw_fonts, native_fonts):
    native_fonts.mkdir()
    for source in sorted(raw_fonts.iterdir()):
        if source.suffix in {".woff", ".woff2"}:
            with TTFont(source) as font:
                font.flavor = None
                font.save(native_fonts / (source.stem + ".ttf"))
        elif source.suffix in {".ttf", ".otf"}:
            shutil.copyfile(source, native_fonts / source.name)
        else:
            raise ValueError(f"Unsupported captured webfont type: {source.suffix}")


def render(work, width, height, target):
    host = work / "native"
    shutil.copytree(TOOLBAR / "native", host)
    generated = subprocess.check_output([
        "cargo", "run", "--locked", "--quiet", "--example", "generate_gpui",
        "--", "--scene-json", str(work / "scene.json"),
    ], cwd=HERE.parents[1])
    (host / "src/generated.rs").write_bytes(generated)
    capture.run("cargo", "+1.95.0", "build", "--locked", "--manifest-path",
                str(host / "Cargo.toml"), "--target-dir", str(target))

    fonts = work / "native-fonts"
    load_fonts(work / "fonts", fonts)
    os.environ["CSS_GPUI_HOST_HEIGHT"] = str(height)
    os.environ["CSS_GPUI_HOST_FONTS"] = str(fonts)
    executable = target / "debug/css-gpui-toolbar-host.exe"
    return capture.capture_window(executable, width, height, work / "native.log"), len(list(fonts.iterdir()))


def make_board(browser, gpui, mask):
    gap, header = 20, 37
    board = Image.new("RGB", (3 * browser.width + 4 * gap, browser.height + header + gap), "#ecebe6")
    draw = ImageDraw.Draw(board)
    font = ImageFont.load_default(size=19)
    for index, (label, frame) in enumerate(zip(
        ("Chrome · original app", "GPUI · generated Rust", "Different pixels"),
        (browser, gpui, mask),
    )):
        x = gap + (gap + browser.width) * index
        draw.text((x, 8), label, fill="#191917", font=font)
        board.paste(frame, (x, header))
    return board


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("noceremony", type=Path, help="path to the running app checkout")
    parser.add_argument("--url", default="http://127.0.0.1:4176/")
    args = parser.parse_args()

    # The native host centers on the same screen as the toolbar comparison.
    scale = capture.user32.GetDpiForSystem() / 96
    metadata = json.loads(capture.run("cargo", "metadata", "--locked", "--no-deps", "--format-version", "1"))
    target = Path(metadata["target_directory"])

    with tempfile.TemporaryDirectory(dir=HERE.parents[1]) as temporary:
        work = Path(temporary)
        capture.run("node", str(HERE / "capture.mjs"), str(args.noceremony.resolve()), args.url, str(work), str(scale))
        scene = json.loads((work / "scene.json").read_text(encoding="utf-8"))
        width = scene["nodes"][0]["layout"]["width"]
        height = scene["nodes"][0]["layout"]["height"]
        (gpui, actual_scale, _), font_count = render(work, width, height, target)
        if scale != actual_scale:
            raise ValueError(f"Browser scale {scale:g} differs from GPUI scale {actual_scale:g}")
        browser = Image.open(work / "browser-reference.png").convert("RGB")
        if browser.size != gpui.size:
            raise ValueError(f"Browser frame {browser.size} differs from GPUI frame {gpui.size}")
        mask, stats = capture.compare(browser, gpui)
        report = json.loads((work / "losses.json").read_text(encoding="utf-8"))
        if font_count == 0:
            raise RuntimeError("The browser capture did not provide its webfonts")
        make_board(browser, gpui, mask).save(HERE / "comparison.png")
        report.update({
            "appRevision": capture.run("git", "-C", str(args.noceremony.resolve()), "rev-parse", "HEAD").strip(),
            "gpui": "0.2.2",
            "logicalViewport": [width, height],
            "framePixels": list(browser.size),
            "nativeNodeCount": len(scene["nodes"]),
            "webfontsLoaded": font_count,
            "pixels": stats,
            "pixelParity": stats["differentPixels"] == 0,
        })
        (HERE / "comparison.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
        print(json.dumps(stats, indent=2))


if __name__ == "__main__":
    main()
