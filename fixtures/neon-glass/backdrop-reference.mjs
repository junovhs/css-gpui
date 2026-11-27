import assert from 'node:assert/strict';
import { mkdir, writeFile } from 'node:fs/promises';
import { createRequire } from 'node:module';
import { resolve } from 'node:path';

const [playwrightRoot, destination] = process.argv.slice(2);
assert(playwrightRoot && destination, 'Usage: node backdrop-reference.mjs <Playwright directory> <output directory>');
const require = createRequire(resolve(playwrightRoot, 'package.json'));
assert.equal(require('playwright/package.json').version, '1.60.0');
const { chromium } = require('playwright');

await mkdir(destination, { recursive: true });
const browser = await chromium.launch({ args: ['--force-color-profile=srgb'] });
try {
    const page = await browser.newPage({ viewport: { width: 256, height: 160 }, deviceScaleFactor: 1 });
    await page.setContent(`
        <style>
            body { margin: 0; }
            #scene {
                position: fixed;
                inset: 0;
                background-image: repeating-linear-gradient(to right, #ff6420 0 56px, #082ae6 56px 111px);
                background-size: 111px 160px;
            }
            #panel {
                position: fixed;
                left: 49px;
                top: 38px;
                width: 158px;
                height: 84px;
                border-radius: 18px;
                backdrop-filter: blur(12px);
                background: #0b0f20a8;
            }
        </style>
        <div id="scene"></div>
        <div id="panel"></div>
    `);
    for (const [index, shift] of [0, 43].entries()) {
        await page.locator('#scene').evaluate((element, shift) => {
            element.style.backgroundPositionX = '-' + shift + 'px';
        }, shift);
        for (const [kind, visibility] of [['scene', 'hidden'], ['blurred', 'visible']]) {
            await page.locator('#panel').evaluate((element, visibility) => {
                element.style.visibility = visibility;
            }, visibility);
            const png = await page.screenshot();
            await writeFile(resolve(destination, index + '-' + kind + '.png'), png);
            const rgba = await page.evaluate(async (base64) => {
                const image = new Image();
                image.src = 'data:image/png;base64,' + base64;
                await image.decode();
                const canvas = document.createElement('canvas');
                canvas.width = image.width;
                canvas.height = image.height;
                const ctx = canvas.getContext('2d');
                ctx.drawImage(image, 0, 0);
                return Array.from(ctx.getImageData(0, 0, image.width, image.height).data);
            }, png.toString('base64'));
            await writeFile(resolve(destination, index + '-' + kind + '.rgba'), Buffer.from(rgba));
        }
    }
} finally {
    await browser.close();
}
