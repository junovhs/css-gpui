// Run under Xvfb to exercise labels on original Chromium markup and generated GPUI.
import assert from 'node:assert/strict';
import { execFileSync, spawn } from 'node:child_process';
import { once } from 'node:events';
import { mkdir, writeFile } from 'node:fs/promises';
import { createRequire } from 'node:module';
import { join, resolve } from 'node:path';
import { setTimeout } from 'node:timers/promises';
import { fileURLToPath, pathToFileURL } from 'node:url';

const [playwright, output] = process.argv.slice(2).map(path => resolve(path));
assert(playwright && output, 'Pass the Playwright installation directory and a new output directory');
const require = createRequire(join(playwright, 'package.json'));
assert.equal(require('playwright/package.json').version, '1.60.0');
const { chromium } = require('playwright');
const root = fileURLToPath(new URL('../../', import.meta.url));
const fixture = join(root, 'benchmarks/css-compat/probes/theme-radio');
const html = join(fixture, 'index.html');
const css = join(fixture, 'style.css');
const host = join(root, 'benchmarks/css-compat/native/Cargo.toml');
const run = (program, args, options) => execFileSync(program, args, {
    cwd: root, encoding: 'utf8', timeout: 60_000, ...options,
});
const [width, height] = [320, 180];
const options = ['light', 'dark'];
const colors = {
    off: ['#ffffff', '#64748b'],
    light: ['#3272ac', '#214f77'],
    dark: ['#3d4a6b', '#242e48'],
};
const steps = [
    { state: 'initial-light', selected: 'light' },
    { state: 'dark-label', selected: 'dark', click: [84, 90] },
    { state: 'light-label', selected: 'light', click: [84, 42] },
    { state: 'right-arrow', selected: 'dark', key: 'Right', browserKey: 'ArrowRight' },
    { state: 'left-arrow', selected: 'light', key: 'Left', browserKey: 'ArrowLeft' },
];

await mkdir(output);
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

const sample = path => JSON.parse(run('python3', ['-c', `
from PIL import Image
import json
import sys

image = Image.open(sys.argv[1]).convert("RGB")
assert image.size == (320, 180), image.size
print(json.dumps([
    ["#%02x%02x%02x" % image.getpixel((x, y)) for x in (36, 25)]
    for y in (42, 90)
]))
`, path]));

const chromiumStates = [];
const browser = await chromium.launch();
try {
    const page = await browser.newPage({ viewport: { width, height } });
    await page.goto(pathToFileURL(html).href);
    for (const { state, selected, click, browserKey } of steps) {
        if (click) {
            const label = page.locator(`label[for="${selected}"]`);
            const bounds = await label.boundingBox();
            assert(bounds && click[0] >= bounds.x && click[0] < bounds.x + bounds.width
                && click[1] >= bounds.y && click[1] < bounds.y + bounds.height);
            await page.mouse.click(...click);
        } else if (browserKey) {
            await page.keyboard.press(browserKey);
        }
        const checked = await page.locator('input[type="radio"][name="theme"]:checked')
            .evaluateAll(inputs => inputs.map(input => input.value));
        const image = join(output, `chromium-${state}.png`);
        await page.screenshot({ path: image });
        chromiumStates.push({ checked, paint: sample(image) });
    }
} finally {
    await browser.close();
}

const nativeStates = [];
const native = spawn(executable, [width, height], { stdio: ['ignore', 'inherit', 'inherit'] });
const exited = once(native, 'exit');
try {
    const xdo = (...args) => run('xdotool', args, { timeout: 15_000 }).trim();
    const window = xdo('search', '--sync', '--onlyvisible', '--pid', String(native.pid)).split('\n')[0];
    xdo('windowfocus', window);
    for (const { state, click, key } of steps) {
        if (click) {
            xdo('mousemove', '--window', window, ...click.map(String), 'click', '1');
        } else if (key) {
            xdo('key', '--clearmodifiers', key);
        }
        await setTimeout(250);
        const image = join(output, `native-${state}.png`);
        run('import', ['-window', window, image], { timeout: 10_000 });
        const paint = sample(image);
        const checked = options.filter((option, index) =>
            paint[index].every((color, channel) => color === colors[option][channel]));
        nativeStates.push({ checked, paint });
    }
} finally {
    native.kill();
    await exited;
}

const results = steps.map(({ state, selected }, index) => ({
    state,
    expected: {
        checked: [selected],
        paint: options.map(option => colors[option === selected ? option : 'off']),
    },
    chromium: chromiumStates[index],
    native: nativeStates[index],
}));
await writeFile(join(output, 'radio-states.json'), JSON.stringify(results, null, 2) + '\n');
for (const { state, expected, chromium, native } of results) {
    assert.deepEqual(chromium, expected, `Chromium ${state} checked option and paint`);
    assert.deepEqual(native, chromium, `GPUI ${state} checked option and paint versus Chromium`);
}
console.log(`Chromium and GPUI agreed on radio selection and paint in all ${steps.length} states.`);
