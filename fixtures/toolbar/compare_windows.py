"""Compare browser and GPUI toolbar frames on Windows.

Run: python fixtures/toolbar/compare_windows.py <project with Playwright>
Requires Chrome, Pillow, windows-capture 2.0.1, PowerShell and Rust 1.95.0.
"""

import argparse
import ctypes
from ctypes import wintypes
import json
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import threading
import time

from PIL import Image, ImageChops, ImageDraw

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
HWND_TOPMOST = wintypes.HWND(-1)
FOREGROUND_FLAGS = 0x0043  # SHOWWINDOW, NOMOVE, NOSIZE

user32 = ctypes.WinDLL("user32", use_last_error=True)
user32.SetProcessDpiAwarenessContext(ctypes.c_void_p(-4))
enum_callback = ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.HWND, wintypes.LPARAM)
user32.EnumWindows.argtypes = [enum_callback, wintypes.LPARAM]
user32.IsWindowVisible.argtypes = [wintypes.HWND]
user32.GetWindowThreadProcessId.argtypes = [wintypes.HWND, ctypes.POINTER(wintypes.DWORD)]
user32.GetClientRect.argtypes = [wintypes.HWND, ctypes.POINTER(wintypes.RECT)]
user32.ClientToScreen.argtypes = [wintypes.HWND, ctypes.POINTER(wintypes.POINT)]
user32.SetWindowPos.argtypes = [wintypes.HWND, wintypes.HWND, *([ctypes.c_int] * 4), wintypes.UINT]
user32.GetDpiForWindow.argtypes = [wintypes.HWND]


def find_window(pid):
    windows = []

    @enum_callback
    def visit(window, _):
        window_pid = wintypes.DWORD()
        user32.GetWindowThreadProcessId(window, ctypes.byref(window_pid))
        if window_pid.value == pid and user32.IsWindowVisible(window):
            windows.append(window)
        return True

    user32.EnumWindows(visit, 0)
    return windows[0] if windows else None


def capture_client(window, origin, width, height):
    from windows_capture import WindowsCapture

    frame_bounds = wintypes.RECT()
    dwm = ctypes.WinDLL("dwmapi")
    if dwm.DwmGetWindowAttribute(wintypes.HWND(window), 9, ctypes.byref(frame_bounds), ctypes.sizeof(frame_bounds)):
        raise RuntimeError("Could not locate visible GPUI window bounds")

    capture = WindowsCapture(window_hwnd=window, cursor_capture=False, draw_border=False)
    ready = threading.Event()
    images = []

    @capture.event
    def on_frame_arrived(frame, control):
        images.append(Image.frombytes(
            "RGB", (frame.width, frame.height), frame.frame_buffer.tobytes(), "raw", "BGRX"
        ))
        ready.set()
        control.stop()

    @capture.event
    def on_closed():
        ready.set()

    control = capture.start_free_threaded()
    if not ready.wait(5):
        control.stop()
        raise RuntimeError("No frame arrived from Windows Graphics Capture")
    control.wait()
    if not images:
        raise RuntimeError("GPUI window closed before capture")

    offset_x = origin.x - frame_bounds.left
    offset_y = origin.y - frame_bounds.top
    image = images[0]
    if not (0 <= offset_x <= image.width - width and 0 <= offset_y <= image.height - height):
        raise ValueError(f"GPUI client region falls outside captured frame {image.size}")
    return image.crop((offset_x, offset_y, offset_x + width, offset_y + height))


def capture_window(executable, width, height, log_path):
    with log_path.open("wb") as log:
        process = subprocess.Popen([str(executable), str(width)], stdout=log, stderr=log)
        try:
            deadline = time.monotonic() + 15
            while time.monotonic() < deadline:
                if process.poll() is not None:
                    break
                window = find_window(process.pid)
                if window:
                    user32.SetWindowPos(window, HWND_TOPMOST, 0, 0, 0, 0, FOREGROUND_FLAGS)
                    time.sleep(0.35)
                    scale = user32.GetDpiForWindow(window) / 96
                    bounds = wintypes.RECT()
                    origin = wintypes.POINT()
                    if not user32.GetClientRect(window, ctypes.byref(bounds)):
                        raise ctypes.WinError(ctypes.get_last_error())
                    if not user32.ClientToScreen(window, ctypes.byref(origin)):
                        raise ctypes.WinError(ctypes.get_last_error())
                    if bounds.right != round(width * scale) or bounds.bottom < round(height * scale):
                        raise ValueError(f"Unexpected GPUI client size {bounds.right}x{bounds.bottom} at {scale:g}x")
                    frame = capture_client(window, origin, round(width * scale), round(height * scale))
                    return frame, scale, log_path.read_text(encoding="utf-8", errors="replace")
                time.sleep(0.05)
            raise RuntimeError(f"GPUI window did not open:\n{log_path.read_text(errors='replace')}")
        finally:
            if process.poll() is None:
                process.terminate()
            process.wait(timeout=10)


def add_probe(source, marker, label):
    if source.count(marker) != 1:
        raise ValueError(f"Expected one generated toolbar marker: {marker}")
    return source.replace(marker, marker + f'''
        .on_children_prepainted(|bounds, _, _| {{
            for (index, bounds) in bounds.into_iter().enumerate() {{
                println!("BOUNDS {label} {{index}} {{}} {{}} {{}} {{}}",
                         f32::from(bounds.origin.x), f32::from(bounds.origin.y),
                         f32::from(bounds.size.width), f32::from(bounds.size.height));
            }}
        }})''')


def read_bounds(log):
    boxes = {}
    for label, index, *values in re.findall(r"^BOUNDS (\w+) (\d+) (\S+) (\S+) (\S+) (\S+)", log, re.M):
        boxes[label, index] = dict(zip(("x", "y", "width", "height"), map(float, values)))
    return {
        "toolbar": boxes["root", "0"],
        "titleBounds": boxes["child", "0"],
        "button": boxes["child", "1"],
    }


def compare(browser, gpui):
    difference = ImageChops.difference(browser, gpui)
    red, green, blue = difference.split()
    nonzero = ImageChops.lighter(red, ImageChops.lighter(green, blue))
    mask = nonzero.point(lambda value: 255 if value else 0)
    markers = Image.new("RGB", browser.size, "#1b2028")
    markers.paste("#ff497c", (0, 0), mask)
    return markers, {
        "differentPixels": mask.histogram()[255],
        "totalPixels": browser.width * browser.height,
        "differentPixelBounds": mask.getbbox(),
    }


def run(*args):
    return subprocess.check_output(args, cwd=ROOT, text=True)


def native_frames(work, browser_cases, target):
    host = work / "native"
    shutil.copytree(HERE / "native", host)
    source_html = (HERE / "source.html").read_text(encoding="utf-8")
    for title in dict.fromkeys(case["title"] for case in browser_cases):
        source = work / "source.html"
        source.write_text(source_html.replace(">Files</span>", f">{title}</span>"), encoding="utf-8")
        generated = run(
            "cargo", "run", "--locked", "--quiet", "--example", "generate_gpui",
            "--", str(source), str(HERE / "source.css"),
        )
        generated = add_probe(generated, ".w_full()", "root")
        generated = add_probe(generated, ".justify_between()", "child")
        (host / "src/generated.rs").write_text(generated, encoding="utf-8")
        run("cargo", "+1.95.0", "build", "--locked", "--manifest-path",
            str(host / "Cargo.toml"), "--target-dir", str(target))
        exe = target / "debug/css-gpui-toolbar-host.exe"
        for case in (item for item in browser_cases if item["title"] == title):
            frame, scale, log = capture_window(exe, case["width"], case["toolbar"]["height"], work / "native.log")
            yield case, frame, scale, read_bounds(log)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("playwright_project", help="directory with Playwright installed")
    args = parser.parse_args()
    references = json.loads((HERE / "browser-bounds.json").read_text(encoding="utf-8"))
    metadata = json.loads(run("cargo", "metadata", "--locked", "--no-deps", "--format-version", "1"))
    target = Path(metadata["target_directory"])
    negative_control = run("pwsh", "-NoProfile", "-File", str(HERE / "verify_negative.ps1"))
    observed = []
    with tempfile.TemporaryDirectory(dir=ROOT) as temporary:
        work = Path(temporary)
        native = list(native_frames(work, references["cases"], target))
        scale = native[0][2]
        if any(frame_scale != scale for _, _, frame_scale, _ in native):
            raise ValueError("Native monitor scale changed mid-run")
        run("node", str(HERE / "capture.mjs"), str(Path(args.playwright_project).resolve()),
            str(scale), str(work))
        references = json.loads((work / "browser-bounds.json").read_text(encoding="utf-8"))
        browser_atlas = Image.open(work / "browser-reference.png").convert("RGB")
        atlas = Image.new("RGB", browser_atlas.size, "#e7eaf0")
        column_width = browser_atlas.width + 16
        row_height = round(64 * scale) + 30
        board = Image.new("RGB", (column_width * 3, row_height * len(native)), "#e7eaf0")
        draw = ImageDraw.Draw(board)
        for row, (case, gpui, _, gpui_bounds) in enumerate(native):
            location = case["atlas"]
            x, y, width, height = (round(location[key] * scale) for key in ("x", "y", "width", "height"))
            browser = browser_atlas.crop((x, y, x + width, y + height))
            markers, pixels = compare(browser, gpui)
            atlas.paste(gpui, (x, y))
            for column, (label, frame) in enumerate(zip(("Browser", "GPUI", "Pixel differences"), (browser, gpui, markers))):
                left, top = 8 + column * column_width, row * row_height
                draw.text((left, top + 6), f'{label} | {case["title"]} | {case["width"]}px', fill="#151922")
                board.paste(frame, (left, top + 24))
            deltas = {}
            for name, bounds in gpui_bounds.items():
                reference = case[name]
                deltas[name] = {
                    "left": bounds["x"] - reference["x"],
                    "top": bounds["y"] - reference["y"],
                    "right": bounds["x"] + bounds["width"] - reference["x"] - reference["width"],
                    "bottom": bounds["y"] + bounds["height"] - reference["y"] - reference["height"],
                }
            observed.append({
                "title": case["title"], "width": case["width"], "gpuiBounds": gpui_bounds,
                "edgeDeltas": deltas, "pixels": pixels,
                "layoutWithinOneLogicalPixel": all(abs(delta) <= 1 for edges in deltas.values() for delta in edges.values()),
            })
    atlas.save(HERE / "gpui-reference.png")
    board.save(HERE / "comparison.png")
    (HERE / "comparison.json").write_text(json.dumps({
        "chrome": references["chrome"], "gpui": "0.2.2", "font": references["font"],
        "deviceScaleFactor": scale,
        "negativeControl": negative_control.splitlines()[0].split("unsupported.css", 1)[-1].lstrip(":"),
        "cases": observed,
    }, indent=2) + "\n", encoding="utf-8")
    for case in observed:
        print(case["title"], case["width"], case["layoutWithinOneLogicalPixel"], case["pixels"])
    if any(not case["layoutWithinOneLogicalPixel"] for case in observed):
        raise SystemExit("Native geometry exceeds one logical pixel. See comparison.json.")


if __name__ == "__main__":
    main()
