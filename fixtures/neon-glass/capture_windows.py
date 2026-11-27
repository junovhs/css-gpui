"""Capture the client area of the standalone Neon Glass Windows executable."""

import argparse
import ctypes as C
from ctypes import wintypes as W
import os
from pathlib import Path
import subprocess
import time

from windows_capture import WindowsCapture
from PIL import Image

u = C.WinDLL('user32')
dwm = C.WinDLL('dwmapi')
u.SetProcessDpiAwarenessContext.argtypes = [C.c_void_p]
u.SetProcessDpiAwarenessContext(C.c_void_p(-4))
u.IsWindowVisible.argtypes = [W.HWND]
u.GetWindowThreadProcessId.argtypes = [W.HWND, C.POINTER(W.DWORD)]
u.GetClientRect.argtypes = [W.HWND, C.POINTER(W.RECT)]
u.ClientToScreen.argtypes = [W.HWND, C.POINTER(W.POINT)]
u.GetDpiForWindow.argtypes = [W.HWND]
dwm.DwmGetWindowAttribute.argtypes = [W.HWND, W.DWORD, C.c_void_p, W.DWORD]
callback = C.WINFUNCTYPE(W.BOOL, W.HWND, W.LPARAM)
u.EnumWindows.argtypes = [callback, W.LPARAM]

def capture(executable, output, width, height, shift):
    args = [str(executable.resolve()), str(shift)]
    if width == 390:
        args.append('--narrow')
    proc = subprocess.Popen(args, cwd=os.environ['TEMP'], env={k: v for k, v in os.environ.items() if k != 'NEON_GLASS_REFERENCE'})
    try:
        handles = []
        @callback
        def find(hwnd, _):
            pid = W.DWORD()
            u.GetWindowThreadProcessId(hwnd, C.byref(pid))
            if pid.value == proc.pid and u.IsWindowVisible(hwnd):
                handles.append(hwnd)
            return True
        for _ in range(100):
            u.EnumWindows(find, 0)
            if handles:
                break
            if proc.poll() is not None:
                raise RuntimeError(f'native executable exited: {proc.returncode}')
            time.sleep(0.1)
        if not handles:
            raise RuntimeError('timed out waiting for the standalone window')
        hwnd = handles[0]
        time.sleep(1.5)
        origin, client, frame_rect = W.POINT(), W.RECT(), W.RECT()
        assert u.ClientToScreen(hwnd, C.byref(origin))
        assert u.GetClientRect(hwnd, C.byref(client))
        assert dwm.DwmGetWindowAttribute(hwnd, 9, C.byref(frame_rect), C.sizeof(frame_rect)) == 0
        scale = u.GetDpiForWindow(hwnd) / 96
        assert abs(client.right - width * scale) <= 1, (client.right, width, scale)
        assert abs(client.bottom - height * scale) <= 1, (client.bottom, height, scale)
        tmp = output.with_name(output.stem + '-wgc.png')
        native = WindowsCapture(cursor_capture=False, draw_border=False, window_hwnd=hwnd)

        @native.event
        def on_frame_arrived(frame, control):
            frame.save_as_image(str(tmp))
            control.stop()

        @native.event
        def on_closed():
            pass

        control = native.start_free_threaded()
        for _ in range(100):
            if control.is_finished():
                break
            time.sleep(0.1)
        else:
            control.stop()
            raise RuntimeError('timed out waiting for a window capture')
        control.wait()
        with Image.open(tmp) as image:
            left = origin.x - frame_rect.left
            top = origin.y - frame_rect.top
            crop = (left, top, left + client.right, top + client.bottom)
            assert crop[0] >= 0 and crop[1] >= 0 and crop[2] <= image.width and crop[3] <= image.height, (crop, image.size)
            image.crop(crop).save(output)
        tmp.unlink()
        print(f'{output.name}: {client.right} x {client.bottom} device pixels, {width} x {height} logical')
    finally:
        proc.terminate()
        proc.wait(timeout=5)

if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('executable', type=Path)
    parser.add_argument('output', type=Path)
    parser.add_argument('width', type=int, choices=(1100, 390))
    parser.add_argument('height', type=int, choices=(760, 844))
    parser.add_argument('shift', type=int)
    args = parser.parse_args()
    capture(args.executable, args.output.resolve(), args.width, args.height, args.shift)
