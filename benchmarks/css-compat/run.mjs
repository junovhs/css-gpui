import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { createRequire } from 'node:module';
import { extname, join, resolve } from 'node:path';

const [playwrightRoot, externalRoot, outputRoot, ...flags] = process.argv.slice(2);
assert(playwrightRoot && externalRoot && outputRoot && flags.every(flag => flag === '--holdout'),
    'Usage: node run.mjs <Playwright install> <cached external assets> <new output directory> [--holdout]');
const require = createRequire(resolve(playwrightRoot, 'package.json'));
const { chromium } = require('playwright');
assert.equal(require('playwright/package.json').version, '1.60.0');

const here = import.meta.dirname;
const repository = resolve(here, '../..');
const lock = await readFile(join(here, 'corpus.json'));
const manifest = JSON.parse(lock);
const subset = flags.includes('--holdout') ? 'holdout' : 'evaluation';
const samples = manifest.samples.filter(sample => sample.subset === subset);
assert(samples.length, `no samples in ${subset}`);
const output = resolve(outputRoot);
assert(output !== repository && !output.startsWith(`${here}/`), 'use an output directory outside the benchmark');
const contentTypes = {
    '.html': 'text/html; charset=utf-8', '.css': 'text/css; charset=utf-8',
    '.js': 'text/javascript; charset=utf-8', '.woff2': 'font/woff2', '.png': 'image/png',
};

const origin = 'http://css-compat.test';
const sha256 = bytes => createHash('sha256').update(bytes).digest('hex');

async function readPinned(path, hash) {
    const bytes = await readFile(path);
    assert.equal(sha256(bytes), hash, `source changed: ${path}`);
    return bytes;
}

// Check all source bytes before running any compilation or browser replay.
for (const [name, hash] of Object.entries(manifest.prompts)) await readPinned(join(here, name), hash);
const sources = new Map();
const externals = new Map();
for (const sample of samples) {
    const files = new Map();
    for (const [name, hash] of Object.entries(sample.files)) {
        files.set(name, await readPinned(join(here, 'corpus', sample.id, name), hash));
    }
    sources.set(sample.id, files);
    for (const url of sample.external ?? []) {
        const file = manifest.external[url];
        externals.set(url, await readPinned(join(resolve(externalRoot), file.name), file.sha256));
    }
}
await mkdir(output);

function cargo(args) {
    const result = spawnSync('cargo', args, { cwd: repository, encoding: 'utf8', maxBuffer: 32 * 1024 * 1024 });
    if (result.error) throw result.error;
    return result;
}

const build = cargo(['build', '--quiet', '--locked', '--example', 'generate_gpui', '--message-format=json']);
assert.equal(build.status, 0, build.stderr);
const executable = build.stdout.trim().split('\n').map(line => JSON.parse(line)).find(message =>
    message.reason === 'compiler-artifact' && message.target.name === 'generate_gpui')?.executable;
assert(executable, 'Cargo did not report the compiler example executable');

const compilerRevision = spawnSync('git', ['rev-parse', 'HEAD'], { cwd: repository, encoding: 'utf8' });
assert.equal(compilerRevision.status, 0, compilerRevision.stderr);
const browser = await chromium.launch();
const report = { subset, corpusSha256: sha256(lock), compilerRevision: compilerRevision.stdout.trim(),
    chromium: browser.version(), results: [] };

try {
    for (const sample of samples) {
        const files = sources.get(sample.id);
        const folder = join(output, sample.id);
        await mkdir(folder);
        const cssInput = join(folder, 'linked.css');
        await writeFile(cssInput, (sample.styles ?? []).map(name =>
            (name.startsWith('https://') ? externals.get(name) : files.get(name)).toString('utf8')).join('\n'));
        const htmlInput = join(here, 'corpus', sample.id, sample.entry);
        for (const [width, height] of manifest.viewports) {
            const args = ['--viewport', `${width}x${height}`, htmlInput, cssInput];
            const runFolder = join(folder, `${width}x${height}`);
            await mkdir(runFolder);
            const strict = await convert(args, runFolder, 'strict');
            const exploratory = strict.clean ? null : await convert(['--allow-loss', ...args], runFolder, 'exploratory');
            const reference = await replay(sample, files, { width, height }, runFolder);
            report.results.push({ sample: sample.id, width, height, strict, exploratory, reference,
                native: { status: strict.clean ? 'not-run' : 'blocked-by-strict-conversion' } });
        }
    }
} finally {
    await browser.close();
}
await writeFile(join(output, 'results.json'), JSON.stringify(report, null, 2) + '\n');
console.log(`${subset}: ${report.results.filter(result => result.strict.clean).length}/${report.results.length} clean conversions`);

async function convert(args, folder, label) {
    const result = spawnSync(executable, args, { encoding: 'utf8', maxBuffer: 32 * 1024 * 1024, timeout: 60_000 });
    const diagnostics = (result.stderr ?? '').replaceAll(here, '<benchmark>').replaceAll(output, '<output>').trim();
    if (diagnostics) await writeFile(join(folder, `${label}.log`), diagnostics + '\n');
    if (result.stdout) await writeFile(join(folder, `${label}.rs`), result.stdout);
    const emitted = result.status === 0 && result.stdout.includes('pub fn render_scene');
    return { exitCode: result.status, signal: result.signal, emitted, clean: emitted && diagnostics === '',
        sourceLosses: diagnostics.split('\n').filter(line => line.includes('source loss in')).length,
        targetLosses: diagnostics.split('\n').filter(line => line.includes('GPUI target loss at')).length,
        error: result.error?.code ?? diagnostics.split('\n').filter(line => line.startsWith('Error:')).at(-1) ?? null };
}

async function replay(sample, files, viewport, folder) {
    const context = await browser.newContext({ viewport, deviceScaleFactor: 1, colorScheme: 'light', reducedMotion: 'no-preference', serviceWorkers: 'block' });
    const page = await context.newPage();
    const errors = [];
    const missing = new Set();
    page.on('pageerror', error => errors.push(error.message));
    await context.route('**/*', async route => {
        const url = new URL(route.request().url());
        const path = url.pathname.slice(1);
        const body = url.origin === origin ? files.get(sample.routes?.[path] ?? path) :
            sample.external?.includes(url.href) && externals.get(url.href);
        const contentType = contentTypes[extname(url.pathname)] ?? 'text/css; charset=utf-8';
        if (body) {
            await route.fulfill({ body, contentType, headers: { 'Access-Control-Allow-Origin': '*' } });
        } else {
            if (['stylesheet', 'font', 'image'].includes(route.request().resourceType()) &&
                !sample.ignoredResources?.some(prefix => url.href.startsWith(prefix))) missing.add(url.href);
            await route.abort();
        }
    });
    const url = `${origin}/${sample.entry}`;
    const open = async () => {
        await page.goto(url);
        if (sample.readySelector) await page.locator(sample.readySelector).waitFor({ state: 'hidden' });
    };
    const capture = async name => {
        await page.evaluate(() => document.fonts.ready);
        await page.screenshot({ path: join(folder, `browser-${name}.png`) });
        return page.evaluate(selectors => Object.fromEntries(selectors.map(selector => {
            const node = document.querySelector(selector);
            if (!node) return [selector, null];
            const rect = node.getBoundingClientRect();
            const style = getComputedStyle(node);
            return [selector, { bounds: [rect.x, rect.y, rect.width, rect.height],
                scroll: [node.scrollLeft, node.scrollTop, node.scrollWidth, node.scrollHeight],
                active: node === document.activeElement, focusVisible: node.matches(':focus-visible'),
                checked: node.checked ?? null, value: typeof node.value === 'string' ? node.value : null,
                display: style.display, flexDirection: style.flexDirection, gridColumns: style.gridTemplateColumns,
                overflowX: style.overflowX, scrollSnapType: style.scrollSnapType, font: style.font,
                foreground: style.color, background: style.backgroundColor,
                outline: style.outline, transform: style.transform, animation: style.animationName,
                transition: style.transitionDuration, theme: node.dataset.theme ?? null }];
        })), sample.probes);
    };
    try {
        await open();
        const baseline = await capture('baseline');
        const states = {};
        for (const scenario of sample.scenarios ?? []) {
            if (scenario.minWidth && viewport.width < scenario.minWidth) continue;
            try {
                await page.evaluate(() => { localStorage.clear(); sessionStorage.clear(); });
                await page.mouse.move(0, 0);
                await page.emulateMedia({ colorScheme: scenario.colorScheme ?? 'light',
                    reducedMotion: scenario.reducedMotion ?? 'no-preference' });
                await open();
                const target = scenario.selector ? page.locator(scenario.selector).first() : null;
                if (scenario.action === 'press') await page.keyboard.press(scenario.key);
                if (scenario.action === 'click') await target.click({ timeout: 1500 });
                if (scenario.action === 'hover') await target.hover({ timeout: 1500 });
                if (scenario.action === 'type') await target.fill(scenario.value, { timeout: 1500 });
                if (scenario.action === 'scroll') {
                    await target.hover({ timeout: 1500 });
                    await page.mouse.wheel(scenario.distance, 0);
                }
                if (scenario.settleMs) await page.waitForTimeout(scenario.settleMs);
                states[scenario.id] = { probes: await capture(scenario.id) };
            } catch (error) {
                states[scenario.id] = { error: error.message };
            }
        }
        return { baseline, states, missingResources: Array.from(missing).sort(), errors: Array.from(new Set(errors)) };
    } finally {
        await context.close();
    }
}
