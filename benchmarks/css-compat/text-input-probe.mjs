// Run under Xvfb. The window is the unchanged benchmark host running generated Rust.
import assert from 'node:assert/strict';
import { execFileSync, spawn } from 'node:child_process';
import { once } from 'node:events';
import { mkdir, writeFile } from 'node:fs/promises';
import { createRequire } from 'node:module';
import { join, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

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
await writeFile(html, '<html><head><link rel="stylesheet" href="probe.css"></head><body><main>' +
    '<input id="first" placeholder="Type a name"><input id="second" type="text" value="Draft">' +
    '</main></body></html>');
await writeFile(css, `
body { margin: 0; background-color: #ffffff; }
main { display: flex; flex-direction: column; gap: 16px; padding: 16px; }
input { display: block; box-sizing: border-box; width: 240px; height: 48px;
    padding: 6px; border: 1px solid #999999; background-color: #ffffff;
    color: #171717; font-family: Arial, sans-serif; font-size: 18px; }
@media (min-width: 800px) { input { width: 400px; } }
`);

const browser = await chromium.launch();
try {
    for (const [width, height] of [[390, 844], [1280, 900]]) {
        const directory = join(output, `${width}x${height}`);
        await mkdir(directory);
        const generated = join(directory, 'generated.rs');
        await writeFile(generated, run('cargo', [
            'run', '--quiet', '--locked', '--example', 'generate_gpui', '--',
            '--viewport', `${width}x${height}`, html, css,
        ]));
        const page = await browser.newPage({ viewport: { width, height } });
        await page.goto(pathToFileURL(html).href);
        const first = page.locator('#first');
        assert.equal(await first.inputValue(), '');
        assert.equal(await page.locator('#second').inputValue(), 'Draft');
        await page.screenshot({ path: join(directory, 'browser-baseline.png') });
        await page.keyboard.press('Tab');
        assert.equal(await page.evaluate(() => document.activeElement.id), 'first');
        await page.screenshot({ path: join(directory, 'browser-focused.png') });
        await page.keyboard.type('Otter');
        await page.keyboard.press('Backspace');
        await page.keyboard.type('r!');
        assert.equal(await first.inputValue(), 'Otter!');
        await page.screenshot({ path: join(directory, 'browser-edited.png') });
        await page.keyboard.press('Tab');
        assert.equal(await page.evaluate(() => document.activeElement.id), 'second');
        await page.keyboard.press('Shift+Tab');
        assert.equal(await page.evaluate(() => document.activeElement.id), 'first');
        assert.equal(await first.inputValue(), 'Otter!');
        const bounds = await first.boundingBox();
        await page.close();

        const build = run('cargo', [
            'build', '--locked', '--manifest-path', host, '--message-format=json-render-diagnostics',
        ], { env: { ...process.env, CSS_GPUI_GENERATED: generated }, timeout: 600_000, maxBuffer: 16 * 1024 ** 2 });
        const executable = build.trim().split('\n').map(JSON.parse)
            .find(message => message.target?.name === 'css-gpui-compat-host' && message.executable).executable;
        const native = spawn(executable, [width, height], { stdio: ['ignore', 'inherit', 'inherit'] });
        const exited = once(native, 'exit');
        try {
            const xdo = (...args) => run('xdotool', args, { timeout: 15_000 }).trim();
            const window = xdo('search', '--sync', '--onlyvisible', '--pid', String(native.pid)).split('\n')[0];
            xdo('windowfocus', window);
            const key = (...args) => xdo('key', '--clearmodifiers', ...args);
            const type = value => xdo('type', '--clearmodifiers', '--', value);
            const pause = () => new Promise(done => setTimeout(done, 250));
            const snapshot = async state => {
                await pause();
                const image = join(directory, `native-${state}.png`);
                run('import', ['-window', window, image]);
                assert.equal(run('identify', ['-format', '%wx%h', image]), `${width}x${height}`);
                const crop = `${bounds.width}x${bounds.height}+${bounds.x}+${bounds.y}`;
                return run('convert', [image, '-crop', crop, '+repage', '-format', '%#', 'info:']);
            };
            const readSelected = async () => {
                key('ctrl+a', 'ctrl+c');
                await pause();
                return run('xclip', ['-selection', 'clipboard', '-o'], { timeout: 5_000 });
            };
            const baseline = await snapshot('baseline');
            key('Tab');
            const focused = await snapshot('focused');
            assert.notEqual(focused, baseline, 'Tab must draw focus in the first field');
            type('Otter');
            key('BackSpace');
            type('r!');
            assert.notEqual(await snapshot('edited'), focused, 'typing must visibly change the first field');
            assert.equal(await readSelected(), 'Otter!');
            key('Tab');
            assert.equal(await readSelected(), 'Draft');
            key('shift+Tab');
            assert.equal(await readSelected(), 'Otter!');
            console.log(`${width}x${height}: browser and native focus order and text agree`);
        } finally {
            native.kill();
            await exited;
        }
    }
} finally {
    await browser.close();
}
