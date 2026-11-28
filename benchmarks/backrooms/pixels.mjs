import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';

export function readPixels(path, width, height) {
    const dimensions = execFileSync('identify', ['-format', '%w %h', path], { encoding: 'utf8' });
    assert.equal(dimensions, `${width} ${height}`, path);
    return execFileSync('convert', [path, '-alpha', 'on', '-depth', '8', 'rgba:-'], {
        maxBuffer: width * height * 4 + 1024,
    });
}

export function compare(native, chromium) {
    let error = 0;
    let changed = 0;
    for (let offset = 0; offset < native.length; offset += 4) {
        const r = Math.abs(native[offset] - chromium[offset]);
        const g = Math.abs(native[offset + 1] - chromium[offset + 1]);
        const b = Math.abs(native[offset + 2] - chromium[offset + 2]);
        error += r + g + b;
        changed += Math.max(r, g, b) >= 8;
    }
    const pixels = native.length / 4;
    return {
        meanAbsoluteRgbError: error / (pixels * 3),
        changedFraction: changed / pixels,
    };
}
