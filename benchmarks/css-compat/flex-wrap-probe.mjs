// Run under Xvfb. Measure the ordinary GPUI host's painted boxes by color.
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

await mkdir(output);
const html = join(output, 'probe.html');
const css = join(output, 'probe.css');
const colors = ['#e02040', '#20a060', '#4050e0'];
await writeFile(html, '<!doctype html><html><head><link rel="stylesheet" href="probe.css">' +
    '</head><body><main>' + colors.map((_, index) => `<div class="child-${index}"></div>`).join('') +
    '</main></body></html>');
await writeFile(css, `
body { margin: 0; }
main { display: flex; flex-wrap: wrap; box-sizing: border-box; width: 100vw;
    padding: 12px; gap: 12px; background-color: #ffffff; }
main div { width: 160px; height: 40px; }
${colors.map((color, index) => `.child-${index} { background-color: ${color}; }`).join('\n')}
`);

const measurePixels = `
from PIL import Image, ImageChops
import json
import sys

image = Image.open(sys.argv[1]).convert('RGB')
bounds = []
for color in sys.argv[2:]:
    difference = ImageChops.difference(image, Image.new('RGB', image.size, color))
    red, green, blue = difference.split()
    mask = ImageChops.lighter(ImageChops.lighter(red, green), blue)
    exact = mask.point(lambda value: 255 if value == 0 else 0)
    box = exact.getbbox()
    if box is None:
        bounds.append(None)
        continue
    left, top, right, bottom = box
    assert exact.histogram()[255] == (right - left) * (bottom - top), color
    bounds.append([left, top, right - left, bottom - top])
print(json.dumps({
    'size': list(image.size),
    'painted': image.getpixel((0, 0)) == (255, 255, 255),
    'bounds': bounds,
}))
`;

const browser = await chromium.launch();
try {
    for (const [width, height] of [[390, 844], [1280, 900]]) {
        const directory = join(output, `${width}x${height}`);
        await mkdir(directory);
        const page = await browser.newPage({ viewport: { width, height } });
        await page.goto(pathToFileURL(html).href);
        const browserBounds = await page.locator('main > div').evaluateAll(children =>
            children.map(child => {
                const { x, y, width, height } = child.getBoundingClientRect();
                return [x, y, width, height];
            }));
        await page.screenshot({ path: join(directory, 'browser.png') });
        await page.close();
        assert.deepEqual(browserBounds, [
            [12, 12, 160, 40],
            [184, 12, 160, 40],
            width === 390 ? [12, 64, 160, 40] : [356, 12, 160, 40],
        ]);

        const generated = join(directory, 'generated.rs');
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
            const image = join(directory, 'native.png');
            let capture;
            for (let attempt = 0; attempt < 30; attempt++) {
                await setTimeout(250);
                run('import', ['-window', window, image], { timeout: 10_000 });
                capture = JSON.parse(run('python3', ['-c', measurePixels, image, ...colors]));
                if (capture.painted) break;
            }
            assert(capture.painted, 'The native window did not paint the row');
            assert.deepEqual(capture.size, [width, height]);
            const result = { browser: browserBounds, native: capture.bounds };
            await writeFile(join(directory, 'bounds.json'), JSON.stringify(result, null, 2) + '\n');
            assert.deepEqual(capture.bounds, browserBounds);
            console.log(`${width}x${height} browser and GPUI child bounds agree: ${JSON.stringify(capture.bounds)}`);
        } finally {
            native.kill();
            await exited;
        }
    }
} finally {
    await browser.close();
}
