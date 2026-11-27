import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { createRequire } from 'node:module';
import { resolve } from 'node:path';

const [playwrightRoot, output] = process.argv.slice(2);
assert(playwrightRoot && output, 'Usage: node capture.mjs <Playwright installation directory> <output directory>');
const require = createRequire(resolve(playwrightRoot, 'package.json'));
const { chromium } = require('playwright');
const playwright = require('playwright/package.json').version;
assert.equal(playwright, '1.60.0');
const sha256 = (bytes) => createHash('sha256').update(bytes).digest('hex');

const sourceSha256 = {
    'source.html': '56554891e03021660f969a56677c104ce550514ba05a8ce0c5c122b6a962f918',
    'source.css': '64fcdc5147d1c642ad101ac4d689470bf494531d5a82df26d261ee5dffc11570',
    'source.js': '6e0bda5ed06176ab65373312caaa2331d3fe4e53b5e392881097da97899fa7d8',
};
const source = {};
for (const [file, digest] of Object.entries(sourceSha256)) {
    const bytes = await readFile(new URL(file, import.meta.url));
    assert.equal(sha256(bytes), digest, file + ' SHA-256');
    source[file] = bytes;
}

// The HTML panel is a body fragment. Serve a standards-mode shell and load the other panels as-is.
const pageUrl = 'http://carousel.test/';
const documentHtml = '<!doctype html>\n<html lang="en">\n' +
    '<meta charset="utf-8">\n<meta name="viewport" content="width=device-width, initial-scale=1">\n' +
    '<title>Carousel - rotate</title>\n<link rel="stylesheet" href="/source.css">\n<body>\n' +
    source['source.html'].toString('utf8') + '<script src="/source.js"></script>\n</body>\n</html>\n';
const resources = new Map([
    [pageUrl, { body: documentHtml, contentType: 'text/html; charset=utf-8' }],
    [pageUrl + 'source.css', { body: source['source.css'], contentType: 'text/css; charset=utf-8' }],
    [pageUrl + 'source.js', { body: source['source.js'], contentType: 'text/javascript; charset=utf-8' }],
    [pageUrl + 'favicon.ico', { status: 204 }],
]);

const viewports = [{ width: 360, height: 280 }, { width: 720, height: 280 }];
const steps = [
    { state: 'initial', click: null, index: 0, angle: null, front: 1 },
    { state: 'next', click: 'Next', index: 1, angle: -40, front: 2 },
    { state: 'previous', click: 'Previous', index: 0, angle: 0, front: 1 },
    { state: 'wrap-previous', click: 'Previous', index: -1, angle: 40, front: 9 },
];

await mkdir(output, { recursive: true });
const browser = await chromium.launch({ args: ['--force-color-profile=srgb'] });
const frames = [];
try {
    for (const viewport of viewports) {
        const context = await browser.newContext({
            viewport,
            deviceScaleFactor: 1,
            locale: 'en-US',
            colorScheme: 'light',
            reducedMotion: 'no-preference',
        });
        const errors = [];
        await context.route('**/*', async (route) => {
            const url = route.request().url();
            const resource = resources.get(url);
            if (resource) {
                await route.fulfill(resource);
            } else {
                errors.push('Unexpected request: ' + url);
                await route.abort();
            }
        });
        const page = await context.newPage();
        page.on('pageerror', (error) => errors.push(String(error)));
        await page.goto(pageUrl);
        await page.evaluate(() => document.fonts.ready);

        const cdp = await context.newCDPSession(page);
        await cdp.send('DOM.enable');
        await cdp.send('CSS.enable');
        const { root } = await cdp.send('DOM.getDocument');
        const { nodeIds } = await cdp.send('DOM.querySelectorAll', {
            nodeId: root.nodeId,
            selector: '.carousel__cell',
        });
        assert.equal(nodeIds.length, 9);

        const readState = async (front) => page.evaluate((number) => {
            const scene = document.querySelector('.scene');
            const carousel = scene.querySelector('.carousel');
            const panel = carousel.children[number - 1];
            const rect = ({ x, y, width, height }) => [x, y, width, height];
            const button = (selector) => rect(document.querySelector(selector).getBoundingClientRect());
            const range = document.createRange();
            range.selectNodeContents(panel);
            return {
                index: window.selectedIndex,
                inlineTransform: carousel.style.transform,
                computedTransform: getComputedStyle(carousel).transform,
                scene: rect(scene.getBoundingClientRect()),
                buttons: { previous: button('.previous-button'), next: button('.next-button') },
                frontTextBounds: rect(range.getBoundingClientRect()),
            };
        }, front);

        for (const { state, click, index, angle, front } of steps) {
            if (click) {
                await page.getByRole('button', { name: click, exact: true }).click();
                await page.mouse.move(0, 0);
            }
            await page.waitForFunction(() => {
                const carousel = document.querySelector('.carousel');
                // Flush the new style so the transition is registered before checking animations.
                getComputedStyle(carousel).transform;
                return carousel.getAnimations().every(({ pending, playState }) =>
                    !pending && playState !== 'running');
            });

            const { index: actualIndex, inlineTransform, ...geometry } = await readState(front);
            assert.deepEqual(errors, []);
            assert.equal(actualIndex, index, state + ' selectedIndex');
            assert.equal(inlineTransform, angle === null ? '' : 'translateZ(-288px) rotateY(' + angle + 'deg)');
            assert.deepEqual(geometry.scene, [(viewport.width - 210) / 2, 40, 210, 140]);
            const cells = await Promise.all(nodeIds.map(async (nodeId, cellIndex) => {
                const { model } = await cdp.send('DOM.getBoxModel', { nodeId });
                assert.equal(model.border.length, 8);
                assert(model.border.every(Number.isFinite));
                return { number: cellIndex + 1, borderQuad: model.border.map((value) => Number(value.toFixed(3))) };
            }));
            const [x, y] = geometry.scene;
            const [left, top] = [x + 11, y + 11];
            const [right, bottom] = [left + 190, top + 120];
            const nominalQuad = [left, top, right, top, right, bottom, left, bottom];
            const frontQuad = cells[front - 1].borderQuad;
            // The panel center is 1px off the carousel's 104px transform origin.
            assert(frontQuad.every((value, axis) => Math.abs(value - nominalQuad[axis]) < 0.5),
                state + ' front panel projection');
            const angledQuad = cells[front % cells.length].borderQuad;
            assert.notEqual(angledQuad[1], angledQuad[3], state + ' projected side panel');

            const { fonts } = await cdp.send('CSS.getPlatformFontsForNode', { nodeId: nodeIds[front - 1] });
            const file = viewport.width + '-' + state + '.png';
            const png = await page.screenshot({ path: resolve(output, file) });
            frames.push({
                file, sha256: sha256(png), viewport, state, click, index, angle, front,
                inlineTransform, ...geometry, frontFonts: fonts, cells,
            });
        }
        await context.close();
    }
    await writeFile(resolve(output, 'browser-reference.json'), JSON.stringify({
        playwright, chromium: browser.version(), platform: process.platform, arch: process.arch,
        deviceScaleFactor: 1, sourceSha256, frames,
    }, null, 2) + '\n');
} finally {
    await browser.close();
}
