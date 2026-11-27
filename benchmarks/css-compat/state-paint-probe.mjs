// Run under Xvfb. Drive the ordinary GPUI host and Chromium with the same state changes.
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
const run = (program, args, options) => execFileSync(program, args, {
    cwd: root, encoding: 'utf8', timeout: 60_000, ...options,
});
const [width, height] = [320, 180];
const inside = [30, 35];
const outside = [280, 140];
const states = [
    ['rest', 'outside', '#345678', '#111111', '#ffffff'],
    ['hover', 'inside', '#177a62', '#cc2222', '#ffee33'],
    ['left', 'outside', '#345678', '#111111', '#ffffff'],
    ['focus', 'Tab', '#773399', '#2233cc', '#bbffcc'],
    ['both', 'inside', '#773399', '#cc2222', '#ffffff'],
    ['hover-after-tab', 'Tab', '#177a62', '#cc2222', '#ffee33'],
    ['both-after-backtab', 'Shift+Tab', '#773399', '#cc2222', '#ffffff'],
    ['focus-after-leaving', 'outside', '#773399', '#2233cc', '#bbffcc'],
    ['blurred', 'Tab', '#345678', '#111111', '#ffffff'],
    ['refocused', 'Shift+Tab', '#773399', '#2233cc', '#bbffcc'],
];

await mkdir(output);
const html = join(output, 'probe.html');
const css = join(output, 'probe.css');
await writeFile(html, `<!doctype html><html><head><link rel="stylesheet" href="probe.css"></head>
<body><main><button id="first" class="probe">MMMM</button><input id="second" type="text" value="Second"></main></body></html>`);
await writeFile(css, `
:root { --hover-fill: #177a62; }
body { margin: 0; }
main { display: flex; flex-direction: column; width: 100vw; height: 100vh;
    padding: 20px; box-sizing: border-box; gap: 12px; background: #ffffff; }
button, input { appearance: none; display: block; box-sizing: border-box;
    width: 168px; height: 52px; padding: 0; border: 4px solid #111111;
    background: #345678; font-family: Arial; font-size: 18px; font-weight: 700; color: #ffffff; }
#first:focus { background-color: #773399; border-color: #2233cc; color: #bbffcc; }
.probe:hover { background: var(--hover-fill); border-color: #cc2222 !important; color: #ffee33; }
#first:focus:hover { color: #ffffff; }
#second:focus { background-color: #c56a26; }
`);

const generated = join(output, 'generated.rs');
// Allow a baseline run to capture the original compiler’s missing state paint.
await writeFile(generated, run('cargo', [
    'run', '--quiet', '--locked', '--example', 'generate_gpui', '--',
    ...(process.env.CSS_GPUI_PROBE_ALLOW_LOSS === '1' ? ['--allow-loss'] : []), '--viewport', `${width}x${height}`, html, css,
]));
const host = join(root, 'benchmarks/css-compat/native/Cargo.toml');
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

const samplePixels = `
from PIL import Image
import json
import sys

image = Image.open(sys.argv[1]).convert("RGB")
assert image.size == (320, 180), image.size
colors = ["#%02x%02x%02x" % image.getpixel(point) for point in [(170, 46), (23, 46)]]
ink = {"#%02x%02x%02x" % pixel for pixel in image.crop((25, 25, 183, 67)).getdata()}
colors += sorted(ink & {"#ffffff", "#ffee33", "#bbffcc"})
colors.append("#%02x%02x%02x" % image.getpixel((170, 110)))
print(json.dumps(colors))
`;
const sample = path => JSON.parse(run('python3', ['-c', samplePixels, path]));
const chromiumColors = [];
const browser = await chromium.launch();
try {
    const page = await browser.newPage({ viewport: { width, height } });
    await page.goto(pathToFileURL(html).href);
    for (const [state, event] of states) {
        if (event === 'inside' || event === 'outside') {
            await page.mouse.move(...(event === 'inside' ? inside : outside));
        } else {
            await page.keyboard.press(event);
        }
        const image = join(output, `chromium-${state}.png`);
        await page.screenshot({ path: image });
        chromiumColors.push(sample(image));
    }
} finally {
    await browser.close();
}

const nativeColors = [];
const native = spawn(executable, [width, height], { stdio: ['ignore', 'inherit', 'inherit'] });
const exited = once(native, 'exit');
try {
    const xdo = (...args) => run('xdotool', args, { timeout: 15_000 }).trim();
    const window = xdo('search', '--sync', '--onlyvisible', '--pid', String(native.pid)).split('\n')[0];
    xdo('windowfocus', window);
    for (const [state, event] of states) {
        if (event === 'inside' || event === 'outside') {
            xdo('mousemove', '--window', window, ...(event === 'inside' ? inside : outside).map(String));
        } else {
            xdo('key', '--clearmodifiers', event === 'Shift+Tab' ? 'shift+Tab' : 'Tab');
        }
        await setTimeout(250);
        const image = join(output, `native-${state}.png`);
        run('import', ['-window', window, image], { timeout: 10_000 });
        nativeColors.push(sample(image));
    }
} finally {
    native.kill();
    await exited;
}

const results = states.map(([state, , background, border, text], index) => ({
    state,
    expected: [background, border, text, ['hover-after-tab', 'blurred'].includes(state) ? '#c56a26' : '#345678'],
    chromium: chromiumColors[index],
    native: nativeColors[index],
}));
await writeFile(join(output, 'state-paint.json'), JSON.stringify(results, null, 2) + '\n');
for (const { state, expected, chromium, native } of results) {
    assert.deepEqual(chromium, expected, `Chromium ${state} control paint`);
    assert.deepEqual(native, chromium, `GPUI ${state} control paint versus Chromium`);
}
console.log(`Chromium and GPUI agreed on control paint in all ${states.length} states.`);
