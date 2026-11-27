// Run under Xvfb. Compare horizontal glyph bounds in Chromium and the ordinary GPUI host.
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

const [width, height] = [390, 240];
const rows = ['start', 'inherited center', 'end', 'centered input'];
const tolerance = 2;
assert.equal(run('fc-match', ['--format=%{family}', 'DejaVu Sans Mono']), 'DejaVu Sans Mono');
await mkdir(output);
const html = join(output, 'probe.html');
const css = join(output, 'probe.css');
await writeFile(html, `<!doctype html><html><head><link rel="stylesheet" href="probe.css">
</head><body><main><div class="start">MMMM</div><div>MMMM</div><div class="end">MMMM</div>
<input type="text" value="MMMM"></main></body></html>`);
await writeFile(css, `
body { margin: 0; text-align: center; }
main { display: flex; flex-direction: column; width: 100vw; height: 100vh;
    box-sizing: border-box; padding: 12px; gap: 8px; background-color: #ffffff; }
main div, main input { width: 216px; height: 36px; padding: 0; color: #000000;
    font-family: 'DejaVu Sans Mono'; font-size: 20px; font-weight: 400; line-height: 36px; }
.start { text-align: start; }
.end { text-align: end; }
main input { appearance: none; border: 0; text-align: center; }
`);

const measurePixels = `
from PIL import Image
import json
import sys

image = Image.open(sys.argv[1]).convert("RGB")
positions = []
for row in range(4):
    y = 12 + row * 44
    region = image.crop((12, y, 228, y + 36))
    ink = region.convert("L").point(lambda value: 255 if value < 128 else 0)
    bounds = ink.getbbox()
    positions.append([bounds[0], bounds[2]] if bounds else None)
print(json.dumps({
    "size": list(image.size),
    "painted": image.getpixel((0, 0)) == (255, 255, 255),
    "positions": positions,
}))
`;
const measure = path => JSON.parse(run('python3', ['-c', measurePixels, path]));
const browserImage = join(output, 'browser.png');
const browser = await chromium.launch();
try {
    const page = await browser.newPage({ viewport: { width, height } });
    await page.goto(pathToFileURL(html).href);
    await page.screenshot({ path: browserImage });
} finally {
    await browser.close();
}
const browserCapture = measure(browserImage);
assert.deepEqual(browserCapture.size, [width, height]);
assert(browserCapture.painted && browserCapture.positions.every(Boolean));
const [start, center, end] = browserCapture.positions;
assert(center[0] - start[0] > 60 && end[0] - center[0] > 60);

const generated = join(output, 'generated.rs');
await writeFile(generated, run('cargo', [
    'run', '--quiet', '--locked', '--example', 'generate_gpui', '--',
    '--viewport', `${width}x${height}`, html, css,
]));
const build = run('cargo', [
    'build', '--locked', '--manifest-path', host, '--message-format=json-render-diagnostics',
], { env: { ...process.env, CSS_GPUI_GENERATED: generated }, timeout: 600_000, maxBuffer: 16 * 1024 ** 2 });
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
        if (capture.painted) break;
    }
    assert(capture.painted, 'The native window did not paint the scene');
    assert.deepEqual(capture.size, [width, height]);

    const results = rows.map((row, index) => ({
        row, browser: browserCapture.positions[index], native: capture.positions[index],
    }));
    await writeFile(join(output, 'positions.json'), JSON.stringify({ tolerance, results }, null, 2) + '\n');
    for (const result of results) {
        assert(result.native && result.native.every((x, edge) =>
            Math.abs(x - result.browser[edge]) <= tolerance), JSON.stringify(result));
    }
    console.log(JSON.stringify(results));
} finally {
    native.kill();
    await exited;
}
