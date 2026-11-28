import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import { createRequire } from 'node:module';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

export const viewports = [[390, 844], [1440, 900]];
export const withoutViewportLighting = 'body > div::before { visibility: hidden !important; }';
const repository = fileURLToPath(new URL('../../', import.meta.url));
const source = 'https://cdpn.io/pen/debug/NPqLLoz';
const sourceHash = '373084e9cf19f2a62b072eed146a687d2879eac74fa1374b0b3ef865836e1c49';
const url = 'http://backrooms.test/';
const run = (program, args, input) => execFileSync(program, args, {
    cwd: repository, timeout: 120_000, maxBuffer: 8 * 1024 * 1024, input,
});

// Keep the context and its own materials. Hide surrounding controls and
// paint from ancestor pseudo-elements while preserving the page backing.
export function isolateContext(page) {
    return page.addStyleTag({
        content: `html::before, html::after, body::before, body::after,
            body > div::before, body > div::after,
            body > div > div::before, body > div > div::after,
            body :not(:has(content), content, content *) { visibility: hidden !important; }`,
    });
}

export async function withBackroomsSource(probe) {
    const html = run('curl', ['--fail', '--silent', '--show-error', '--location', '--max-time', '60', source]);
    assert.equal(createHash('sha256').update(html).digest('hex'), sourceHash, 'Pinned HTML has changed');

    const build = run('cargo', ['build', '--locked', '--release', '--example', 'inspect', '--message-format=json']);
    const inspect = build.toString().trim().split('\n').map(line => JSON.parse(line))
        .find(message => message.reason === 'compiler-artifact' && message.target.name === 'inspect')?.executable;
    assert(inspect, 'Cargo did not report the inspect executable');

    const directory = await mkdtemp(join(tmpdir(), 'css-gpui-backrooms-'));
    try {
        const htmlPath = join(directory, 'input.html');
        const cssPath = join(directory, 'linked.css');
        await writeFile(htmlPath, html);
        await writeFile(cssPath, '');
        const snapshot = (mode, width, height, selection, input, linkedCss = cssPath) => JSON.parse(run(inspect,
            [mode, `${width}x${height}`, htmlPath, linkedCss, ...selection], input));
        await probe({ html, htmlPath, cssPath, directory, snapshot });
    } finally {
        await rm(directory, { recursive: true, force: true });
    }
}

export async function withBackrooms(probe) {
    const playwright = process.argv[2];
    assert(playwright, 'Pass the directory containing the Playwright 1.60.0 installation');
    const require = createRequire(join(resolve(playwright), 'package.json'));
    assert.equal(require('playwright/package.json').version, '1.60.0');
    const { chromium } = require('playwright');
    await withBackroomsSource(async ({ html, htmlPath, cssPath, directory, snapshot }) => {
        const browser = await chromium.launch();
        try {
            const newPage = async () => {
                const page = await browser.newPage();
                await page.route('**/*', route => route.request().url() === url
                    ? route.fulfill({ body: html, contentType: 'text/html',
                        headers: { 'Content-Security-Policy': "script-src 'none'" } })
                    : route.abort());
                return page;
            };
            const page = await newPage();
            const reset = async (width, height, target = page) => {
                await target.setViewportSize({ width, height });
                await target.goto(url, { waitUntil: 'domcontentloaded' });
                await target.addStyleTag({ content: '* { transition: none !important; animation: none !important; }' });
            };
            await probe({ page, newPage, snapshot, reset, htmlPath, cssPath, directory });
        } finally {
            await browser.close();
        }
    });
}
