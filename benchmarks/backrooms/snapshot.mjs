import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import { createRequire } from 'node:module';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const playwright = process.argv[2];
assert(playwright, 'Pass the directory containing the Playwright 1.60.0 installation');
const require = createRequire(join(resolve(playwright), 'package.json'));
assert.equal(require('playwright/package.json').version, '1.60.0');
const { chromium } = require('playwright');
const repository = fileURLToPath(new URL('../../', import.meta.url));
const source = 'https://cdpn.io/pen/debug/NPqLLoz';
const sourceHash = '373084e9cf19f2a62b072eed146a687d2879eac74fa1374b0b3ef865836e1c49';
const viewports = [[390, 844], [1440, 900]];
const steps = [[], ['play'], ['play', 'z-8'], ['play', 'z-8', 'right1'],
    ['play', 'z-8', 'right1', 'level-2']];
const run = (program, args) => execFileSync(program, args, {
    cwd: repository, timeout: 120_000, maxBuffer: 8 * 1024 * 1024,
});
const html = run('curl', ['--fail', '--silent', '--show-error', '--location', '--max-time', '60', source]);
assert.equal(createHash('sha256').update(html).digest('hex'), sourceHash, 'Pinned HTML has changed');

const build = run('cargo', [
    'build', '--locked', '--release', '--example', 'inspect', '--message-format=json',
]);
const inspect = build.toString().trim().split('\n').map(line => JSON.parse(line))
    .find(message => message.reason === 'compiler-artifact' && message.target.name === 'inspect')?.executable;
assert(inspect, 'Cargo did not report the inspect executable');

const directory = await mkdtemp(join(tmpdir(), 'css-gpui-backrooms-snapshot-'));
try {
    const htmlPath = join(directory, 'input.html');
    const cssPath = join(directory, 'linked.css');
    await writeFile(htmlPath, html);
    await writeFile(cssPath, '');

    const browser = await chromium.launch();
    try {
        const page = await browser.newPage();
        const url = 'http://backrooms.test/';
        await page.route('**/*', route => route.request().url() === url
            ? route.fulfill({ body: html, contentType: 'text/html',
                headers: { 'Content-Security-Policy': "script-src 'none'" } })
            : route.abort());
        for (const [width, height] of viewports) {
            await page.setViewportSize({ width, height });
            await page.goto(url, { waitUntil: 'domcontentloaded' });
            await page.addStyleTag({ content: '* { transition: none !important; animation: none !important; }' });

            for (const selection of steps) {
                const actual = JSON.parse(run(inspect, [
                    '--radio-snapshot', `${width}x${height}`, htmlPath, cssPath, ...selection,
                ]));
                const browserSnapshot = await page.evaluate(ids => {
                    for (const id of ids) document.getElementById(id).checked = true;
                    const style = (selector, pseudo) => getComputedStyle(document.querySelector(selector), pseudo);
                    const transform = selector => {
                        const value = style(selector).transform;
                        return new DOMMatrixReadOnly(value === 'none' ? undefined : value);
                    };
                    const turn = transform('content > w > x');
                    const background = style('html').backgroundColor.match(/\d+/g);
                    return {
                        menuOffset: transform('buttons').m41,
                        overlayPosition: style('bh').position,
                        walk: transform('content > w > x > y > z').m43,
                        turn: Math.atan2(turn.m31, turn.m11) * 180 / Math.PI,
                        background: '#' + background.map(channel => Number(channel).toString(16).padStart(2, '0')).join(''),
                        buttons: [...document.querySelectorAll('buttons > label')]
                            .filter(node => getComputedStyle(node).display !== 'none')
                            .map(node => [node.className, node.htmlFor]),
                        levelLabel: style('label[for="level-2"]').display,
                        levelLabelContent: style('label[for="level-2"]', '::before').content,
                    };
                }, selection);
                const resolvedSnapshot = readResolvedSnapshot(actual);
                const name = `${width}x${height}, ${selection.join(' ') || 'authored defaults'}`;
                assert.deepEqual(resolvedSnapshot, browserSnapshot, name);
                console.log(JSON.stringify({ width, height, selection, ...resolvedSnapshot }));
            }
        }
    } finally {
        await browser.close();
    }
} finally {
    await rm(directory, { recursive: true, force: true });
}

function readResolvedSnapshot(nodes) {
    const first = tag => nodes.find(node => node.tag === tag);
    const transform = node => node.projection.transform;
    const translation = (node, axis) => {
        const value = transform(node);
        if (value === undefined || value === 'none') return 0;
        const args = /^translate3d\(([^)]+)\)$/.exec(value)?.[1];
        assert(args, `Expected a single translate3d for <${node.tag}>, received ${value}`);
        const coordinate = args.split(',')[axis].trim();
        if (coordinate === '0') return 0;
        assert(coordinate.endsWith('rem'), `Expected a rem translation, received ${coordinate}`);
        return Number(coordinate.slice(0, -3)) * (first('html').font_size ?? 16);
    };
    const turn = transform(first('x'));
    const degrees = turn === undefined || turn === 'none' ? 0 : /^rotateY\((-?\d+)deg\)$/.exec(turn)?.[1];
    assert(degrees !== undefined, `Expected a single rotateY, received ${turn}`);
    const buttons = first('buttons');
    const label = nodes.find(node => node.tag === 'label' && node.attributes.for === 'level-2');
    return {
        menuOffset: translation(buttons, 0),
        overlayPosition: first('bh').position,
        walk: translation(first('z'), 2),
        turn: Number(degrees),
        background: first('html').background_color,
        buttons: nodes.filter(node => node.parent_id === buttons.id && node.tag === 'label' && node.display !== 'none')
            .map(node => [node.attributes.class, node.attributes.for]),
        levelLabel: label.display,
        levelLabelContent: label.decorations.find(decoration => decoration.pseudo === 'before').effects.content,
    };
}
