// Run under Xvfb. Compare Chromium label advance with the actual GPUI window.
import assert from 'node:assert/strict';
import { execFileSync, spawn } from 'node:child_process';
import { once } from 'node:events';
import { mkdir, writeFile } from 'node:fs/promises';
import { createRequire } from 'node:module';
import { join, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { setTimeout } from 'node:timers/promises';

const [playwright, output] = process.argv.slice(2).map(path => resolve(path));
assert(playwright && output, 'Pass the Playwright installation directory and a new output directory');
const require = createRequire(join(playwright, 'package.json'));
assert.equal(require('playwright/package.json').version, '1.60.0');
const { chromium } = require('playwright');
const root = fileURLToPath(new URL('../../', import.meta.url));
const host = join(root, 'benchmarks/css-compat/native/Cargo.toml');
const run = (program, args, options) => execFileSync(program, args, {
    cwd: root, encoding: 'utf8', timeout: 60_000, ...options,
});

const [width, height] = [340, 150];
const rows = ['control', 'positive', 'negative'];
const tolerance = 1;
assert.equal(run('fc-match', ['--format=%{family}', 'DejaVu Sans Mono']), 'DejaVu Sans Mono');
await mkdir(output);
const html = join(output, 'probe.html');
const css = join(output, 'probe.css');
const label = 'MMMMMM';
const markup = rows.map(row =>
    `<div class="row"><div class="label ${row}">${label}</div><div class="marker"></div></div>`).join('\n');
await writeFile(html, `<!doctype html><html><head><link rel="stylesheet" href="probe.css">
</head><body><main>${markup}</main></body></html>`);
await writeFile(css, `
body { margin: 0; }
main { display: flex; flex-direction: column; width: 100vw; height: 100vh;
    box-sizing: border-box; padding: 12px; gap: 8px; background-color: #ffffff; }
.row { display: flex; align-items: center; height: 32px; }
.label { color: #000000; font-family: 'DejaVu Sans Mono'; font-size: 20px;
    font-weight: 400; line-height: 32px; }
.positive { letter-spacing: 3px; }
.negative { letter-spacing: -0.1em; }
.marker { width: 3px; height: 12px; background-color: #ff0000; }
`);

const measurePixels = `
from PIL import Image
import json
import sys

image = Image.open(sys.argv[1]).convert("RGB")
rows = []
for row in range(3):
    y = 12 + row * 40
    region = image.crop((12, y, 320, y + 32))
    marker_x = next((x for x in range(region.width) if region.getpixel((x, 16)) == (255, 0, 0)), None)
    ink = Image.new("L", region.size)
    ink.putdata([255 if max(pixel) < 128 else 0 for pixel in region.getdata()])
    bounds = ink.getbbox()
    rows.append({"advance": marker_x, "inkRight": bounds[2] if bounds else None})
print(json.dumps({"size": list(image.size), "rows": rows}))
`;
const measure = path => JSON.parse(run('python3', ['-c', measurePixels, path]));
const browserImage = join(output, 'browser.png');
const browser = await chromium.launch();
let advances;
try {
    const page = await browser.newPage({ viewport: { width, height } });
    await page.goto(pathToFileURL(html).href);
    await page.evaluate(() => document.fonts.ready.then(() => undefined));
    advances = await page.locator('.label').evaluateAll(labels =>
        labels.map(label => label.getBoundingClientRect().width));
    await page.screenshot({ path: browserImage });
} finally {
    await browser.close();
}
const browserCapture = measure(browserImage);
assert.deepEqual(browserCapture.size, [width, height]);
assert.equal(advances[1] - advances[0], 3 * label.length);
assert.equal(advances[2] - advances[0], -2 * label.length);

const generated = join(output, 'generated.rs');
await writeFile(generated, run('cargo', [
    'run', '--quiet', '--locked', '--example', 'generate_gpui', '--',
    '--viewport', `${width}x${height}`, html, css,
]));
const build = run('cargo', [
    'build', '--locked', '--manifest-path', host, '--message-format=json-render-diagnostics',
], {
    env: {
        ...process.env,
        ...(process.env.CSS_GPUI_NATIVE_TARGET_DIR && { CARGO_TARGET_DIR: process.env.CSS_GPUI_NATIVE_TARGET_DIR }),
        CSS_GPUI_GENERATED: generated,
    },
    timeout: 600_000,
    maxBuffer: 16 * 1024 ** 2,
});
const executable = build.trim().split('\n').map(JSON.parse)
    .find(message => message.target?.name === 'css-gpui-compat-host' && message.executable).executable;
const native = spawn(executable, [width, height], { stdio: ['ignore', 'inherit', 'inherit'] });
const exited = once(native, 'exit');
try {
    const window = run('xdotool', ['search', '--sync', '--onlyvisible', '--pid', String(native.pid)],
        { timeout: 15_000 }).trim().split('\n')[0];
    run('xdotool', ['windowfocus', window]);
    const nativeImage = join(output, 'native.png');
    let capture;
    for (let attempt = 0; attempt < 30; attempt++) {
        await setTimeout(250);
        run('import', ['-window', window, nativeImage], { timeout: 10_000 });
        capture = measure(nativeImage);
        if (capture.rows.every(row => row.advance !== null && row.inkRight !== null)) break;
    }
    assert.deepEqual(capture.size, [width, height]);
    assert(capture.rows.every(row => row.advance !== null && row.inkRight !== null),
        'The native window did not paint all labels and advance markers');
    assert(browserCapture.rows.every(row => row.inkRight !== null));
    const results = rows.map((row, index) => ({
        row,
        browserAdvance: advances[index],
        nativeAdvance: capture.rows[index].advance,
        browserInkRight: browserCapture.rows[index].inkRight,
        nativeInkRight: capture.rows[index].inkRight,
    }));
    await writeFile(join(output, 'advances.json'), JSON.stringify({ tolerance, results }, null, 2) + '\n');
    const [control, ...spaced] = results;
    assert(Math.abs(control.nativeAdvance - control.browserAdvance) <= tolerance, JSON.stringify(control));
    for (const result of spaced) {
        for (const [native, browser] of [['nativeAdvance', 'browserAdvance'], ['nativeInkRight', 'browserInkRight']]) {
            assert(result[native] !== null && result[browser] !== null);
            const nativeDelta = result[native] - control[native];
            const browserDelta = result[browser] - control[browser];
            assert(Math.abs(nativeDelta - browserDelta) <= tolerance,
                `${result.row} ${native}: Chromium Δ${browserDelta}, GPUI Δ${nativeDelta}`);
        }
    }
    console.log(JSON.stringify(results));
} finally {
    native.kill();
    await exited;
}
