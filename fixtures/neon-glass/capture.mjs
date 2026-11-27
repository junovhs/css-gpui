import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { createRequire } from 'node:module';
import { resolve } from 'node:path';

const [playwrightRoot, outputPath, ...flags] = process.argv.slice(2);
const nativeFonts = flags.includes('--native');
const recordEffects = flags.includes('--effects') || nativeFonts;
assert(playwrightRoot && outputPath && flags.every((flag) => ['--effects', '--native'].includes(flag)),
    'Usage: node capture.mjs <Playwright installation directory> <output directory> [--effects|--native]');
const require = createRequire(resolve(playwrightRoot, 'package.json'));
const { chromium } = require('playwright');
assert.equal(require('playwright/package.json').version, '1.60.0');

const output = resolve(outputPath);
await mkdir(output, { recursive: true });
const readFixture = (name) => readFile(new URL(name, import.meta.url));
const reference = JSON.parse(await readFixture('reference.json'));
const source = Buffer.from((await readFixture('source.html')).toString('utf8').replaceAll('\r\n', '\n'));
const pageUrl = 'http://neon-glass.test/';
const normalizeUrl = 'https://public.codepenassets.com/css/normalize-5.0.0.min.css';
const sharedCssUrl = 'https://codepen.io/simeydotme/pen/jEOaeNd/63809857734821de257097bf620e7121.css';
const sharedFrameUrl = sharedCssUrl.replace('codepen.io/simeydotme/pen/', 'cdpn.io/simeydotme/fullpage/');
const sha256 = (bytes) => createHash('sha256').update(bytes).digest('hex');

assert.equal(sha256(source), reference.sourceSha256, 'Pen source SHA-256');

const resources = new Map();
async function fetchResource(url) {
    const expected = reference.assets[url];
    assert(expected, 'Unpinned resource: ' + url);
    const response = await fetch(url, { signal: AbortSignal.timeout(30_000) });
    assert(response.ok, url + ' returned HTTP ' + response.status);
    const body = Buffer.from(await response.arrayBuffer());
    assert.equal(sha256(body), expected, url);
    return { body, contentType: response.headers.get('content-type') };
}

async function serveResource(url) {
    if (url === pageUrl) return { body: source, contentType: 'text/html; charset=utf-8' };
    if (url === sharedCssUrl) {
        const { body } = await fetchResource(sharedFrameUrl);
        const frame = body.toString('utf8').match(/srcdoc="([^"]+)"/)?.[1];
        const css = frame?.match(/<style>([\s\S]*?)<\/style>/)?.[1];
        assert(css, 'Shared Pen stylesheet is missing from the renderer');
        // The full-page renderer exposes this Pen as HTML containing a CSS block.
        return { body: css.replace(/&(quot|amp);/g, (_, name) => name === 'quot' ? '"' : '&'), contentType: 'text/css; charset=utf-8' };
    }
    return fetchResource(url);
}

async function settle(page) {
    await page.waitForFunction(() =>
        document.fonts.status === 'loaded' &&
        document.getAnimations().every(({ pending, playState }) => !pending && playState !== 'running')
    );
}

async function sampleRgb(page, png) {
    return page.evaluate(async (base64) => {
        const grid = 20;
        const image = new Image();
        image.src = 'data:image/png;base64,' + base64;
        await image.decode();
        const canvas = document.createElement('canvas');
        canvas.width = image.width;
        canvas.height = image.height;
        const ctx = canvas.getContext('2d');
        ctx.drawImage(image, 0, 0);
        const { data } = ctx.getImageData(0, 0, canvas.width, canvas.height);
        const samples = [];
        for (let row = 0; row < grid; row++) {
            for (let col = 0; col < grid; col++) {
                const x = Math.floor((col + 0.5) * canvas.width / grid);
                const y = Math.floor((row + 0.5) * canvas.height / grid);
                const offset = 4 * (y * canvas.width + x);
                samples.push(...data.subarray(offset, offset + 3));
            }
        }
        return samples;
    }, png.toString('base64'));
}

const browser = await chromium.launch({ args: ['--force-color-profile=srgb', '--disable-gpu', '--run-all-compositor-stages-before-draw'] });
const errors = [];
const frames = [];
const geometry = [];
const effects = [];
try {
    for (const viewport of [{ width: 1100, height: 760 }, { width: 390, height: 844 }]) {
        const context = await browser.newContext({ viewport, deviceScaleFactor: 1, locale: 'en-US', colorScheme: 'dark' });
        await context.route('**/*', async (route) => {
            const url = route.request().url();
            try {
                if (!resources.has(url)) resources.set(url, serveResource(url));
                await route.fulfill({
                    ...await resources.get(url),
                    headers: { 'access-control-allow-origin': '*' },
                });
            } catch (error) {
                errors.push(String(error));
                await route.abort();
            }
        });
        await context.addInitScript(() => {
            localStorage.setItem('neon-glass-hue1', '255');
            localStorage.setItem('neon-glass-hue2', '222');
        });

        const page = await context.newPage();
        page.on('pageerror', (error) => errors.push(String(error)));
        await page.goto(pageUrl);
        await page.evaluate(() => document.fonts.ready);
        assert.deepEqual(errors, []);
        const boxes = await page.evaluate(() => {
            const selectors = {
                menu: '#menu',
                content: '#menu .inner',
                search: '#menu .search',
                searchInput: '#menu .search input',
                firstRow: '#menu li',
                lastRow: '#menu section:last-of-type li:last-child',
                separator: '#menu hr',
            };
            return Object.fromEntries(Object.entries(selectors).map(([name, selector]) => {
                const { x, y, width, height } = document.querySelector(selector).getBoundingClientRect();
                return [name, [x, y, width, height]];
            }));
        });
        const rows = await page.locator('#menu li').evaluateAll((elements) => elements.map((element) => {
            const { x, y, width, height } = element.getBoundingClientRect();
            return { label: element.textContent.trim(), bounds: [x, y, width, height] };
        }));
        geometry.push({ ...viewport, boxes, rows });
        const menu = page.locator('#menu');
        const row = menu.getByText('Calculator', { exact: true });
        const positions = [
            { name: 'upper-left', x: viewport.width < 480 ? 16 : 80, y: 120 },
            { name: 'lower-right', x: viewport.width - 8, y: viewport.height - 8 },
        ];
        for (const { name, x, y } of positions) {
            const capture = async (state) => {
                const file = viewport.width + '-' + name + '-' + state + '.png';
                const png = await page.screenshot({ animations: 'disabled', path: resolve(output, file) });
                const hash = sha256(png);
                const expected = reference.frames[frames.length];
                assert.equal(file, expected.file);
                if (process.platform === reference.platform && hash !== expected.sha256) {
                    // Narrow hover frames can vary slightly during Chromium compositing.
                    assert(expected.rgbSamples, file + ' SHA-256 mismatch');
                    const baseline = Buffer.from(expected.rgbSamples, 'base64');
                    const samples = await sampleRgb(page, png);
                    assert.equal(samples.length, baseline.length, file + ' sample count');
                    const maxDelta = Math.max(...samples.map((channel, index) => Math.abs(channel - baseline[index])));
                    assert(maxDelta <= 3, file + ' sampled RGB delta: ' + maxDelta);
                }
                frames.push({ file, sha256: hash });
            };

            await page.mouse.click(2, 2);
            await menu.waitFor({ state: 'hidden' });
            await page.mouse.click(x, y, { button: 'right' });
            await page.mouse.move(2, 2);
            await menu.waitFor({ state: 'visible' });
            await settle(page);

            const bounds = await menu.boundingBox();
            assert(Math.abs(bounds.x - Math.min(x, viewport.width - bounds.width - 30)) < 0.1);
            assert(Math.abs(bounds.y - Math.min(y, viewport.height - bounds.height - 20)) < 0.1);
            await capture('open');

            await row.hover();
            await settle(page);
            assert(await row.evaluate((element) => element.matches(':hover')));
            await capture('hover');

            await page.mouse.click(2, 2);
            await menu.waitFor({ state: 'hidden' });
            await settle(page);
            await capture('closed');
        }
        if (recordEffects && viewport.width === 1100) {
            await page.addStyleTag({
                content: '#app, .social-icon, #menu .inner { opacity: 0 !important; pointer-events: none !important }',
            });
            const placements = [
                { shift: 0, x: 80, y: 120 },
                { shift: 90, x: 1092, y: 752 },
            ];
            for (const [index, { shift, x, y }] of placements.entries()) {
                await page.mouse.click(2, 2);
                await menu.waitFor({ state: 'hidden' });
                await page.evaluate((shift) => {
                    document.body.style.backgroundPosition = 'calc(50% + ' + shift + 'px) center, center';
                }, shift);
                await settle(page);
                await page.screenshot({
                    path: resolve(output, 'chrome-stage-' + index + '.png'),
                    animations: 'disabled',
                });
                await page.mouse.click(x, y, { button: 'right' });
                await page.mouse.move(2, 2);
                await menu.waitFor({ state: 'visible' });
                await settle(page);
                const panel = await menu.boundingBox();
                await page.screenshot({
                    path: resolve(output, 'chrome-effects-' + index + '.png'),
                    animations: 'disabled',
                });
                effects.push({ ...viewport, shift, panel });
            }
        }
        await context.close();
    }
    assert.deepEqual(errors, []);
    const chromiumVersion = browser.version();
    assert.equal(chromiumVersion, reference.chromium, 'Chromium version');
    const shared = (await resources.get(sharedCssUrl)).body;
    const normalize = (await resources.get(normalizeUrl)).body;
    await writeFile(resolve(output, 'shared.css'), shared);
    await writeFile(resolve(output, 'linked.css'), Buffer.concat([normalize, Buffer.from('\n'), Buffer.from(shared)]));
    await writeFile(resolve(output, 'geometry.json'), JSON.stringify(geometry, null, 2) + '\n');
    if (recordEffects) {
        for (const file of ['abstract-light.jpg', 'noise-base.png']) {
            const url = 'https://assets.codepen.io/13471/' + file;
            await writeFile(resolve(output, file), (await resources.get(url)).body);
        }
        await writeFile(resolve(output, 'effects.json'), JSON.stringify(effects, null, 2) + '\n');
    }
    if (nativeFonts) {
        assert.equal(require('wawoff2/package.json').version, '2.0.1');
        const { decompress } = require('wawoff2');
        const fonts = resolve(output, 'fonts');
        await mkdir(fonts, { recursive: true });
        for (const url of Object.keys(reference.assets).filter((url) => url.endsWith('.woff2'))) {
            const { body } = resources.has(url) ? await resources.get(url) : await fetchResource(url);
            const file = url.split('/').at(-1).replace(/\.woff2$/, '.ttf');
            await writeFile(resolve(fonts, file), await decompress(body));
        }
    }
    await writeFile(resolve(output, 'frames.json'), JSON.stringify({
        platform: process.platform, chromium: chromiumVersion, sourceSha256: sha256(source), frames,
    }, null, 2) + '\n');

    if (process.platform === reference.platform) {
        assert.equal(frames.length, reference.frames.length);
        console.log('Verified ' + frames.length + ' reference frames against the pinned baseline.');
    } else {
        console.log('Captured ' + frames.length + ' frames. Pixel hashes are pinned for ' + reference.platform + '.');
    }
} finally {
    await browser.close();
}
