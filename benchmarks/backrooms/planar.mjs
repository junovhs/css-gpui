import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { viewports, withBackrooms } from './probe.mjs';

const executable = process.argv[3] && resolve(process.argv[3]);
assert(executable, 'Pass the css-gpui-planar-preview binary as the second argument');
const captures = process.argv[4] && resolve(process.argv[4]);
const capture = fileURLToPath(new URL('./capture-planar.sh', import.meta.url));
const steps = [
    ['enter', ['play']],
    ['walk', ['play', 'z-8']],
    ['turn', ['play', 'z-8', 'right1']],
];

const cornerTiles = [6, 7, 8, 9].flatMap(column =>
    [6, 7, 8, 9, 10].map(row => `body content floor > tile.col-${column}.row-${row}`));
const coverageCss = `
    html { background-color: #000 !important; }
    body content, body content *, body content *::before, body content *::after {
        color: transparent !important;
        background-color: transparent !important;
        background-image: none !important;
        box-shadow: none !important;
        outline: none !important;
    }
    ${cornerTiles.map(tile => `${tile}::before`).join(', ')} {
        background-color: #f00 !important; filter: none !important; opacity: 1 !important;
    }
    ${cornerTiles.map(tile => `${tile}::after`).join(', ')} {
        background-color: #0f0 !important; filter: none !important; opacity: 1 !important;
    }
`;

await withBackrooms(async ({ page, snapshot, reset, directory }) => {
    const out = captures ?? directory;
    await mkdir(out, { recursive: true });
    const coverageFile = join(directory, 'coverage.css');
    await writeFile(coverageFile, coverageCss);
    for (const [width, height] of viewports) {
        let previous;
        for (const [name, radios] of steps) {
            await reset(width, height);
            await page.addStyleTag({
                content: '*::before, *::after { transition: none !important; animation: none !important; }',
            });
            await page.evaluate(ids => {
                for (const id of ids) document.getElementById(id).checked = true;
            }, radios);
            const prefix = join(out, `${width}x${height}-${name}`);
            const rgba = `${prefix}.rgba`;
            const { planes, losses } = snapshot(
                '--planar-preview', width, height, radios, JSON.stringify({ root: 'content', rgba }),
            );
            if (captures) await writeFile(`${prefix}-planes.json`, JSON.stringify({ planes, losses }, null, 2));
            const authored = `${prefix}-authored.png`;
            const isolated = `${prefix}-isolated.png`;
            await page.screenshot({ path: authored });
            // Hide siblings of the context and paint from its ancestor
            // pseudo-elements. Keep the whole context, including its own
            // backgrounds, textures, filters and shadows, in the comparison.
            await page.addStyleTag({
                content: `html::before, html::after, body::before, body::after,
                    body > div::before, body > div::after,
                    body > div > div::before, body > div > div::after,
                    body :not(:has(content), content, content *) { visibility: hidden !important; }`,
            });
            await page.screenshot({ path: isolated });

            const actual = await readNative(rgba, `${prefix}-native.png`, width, height);
            const reference = readPixels(isolated, width, height);
            const fullPage = compare(actual, readPixels(authored, width, height));
            const context = compare(actual, reference);
            assert(planes.length >= 20, `${name} ${width}: projected too few painted faces`);
            assert(planes.every(({ polygon }) => polygon.every(([x, y]) =>
                x >= -1e-6 && y >= -1e-6 && x <= width + 1e-6 && y <= height + 1e-6)),
            `${name} ${width}: visible polygon escaped the viewport`);
            assert(context.meanAbsoluteRgbError <= 13,
                `${name} ${width}: isolated context RGB error ${context.meanAbsoluteRgbError}`);
            const coverageRgba = `${prefix}-coverage.rgba`;
            snapshot('--planar-preview', width, height, radios,
                JSON.stringify({ root: 'content', rgba: coverageRgba }), coverageFile);
            await page.addStyleTag({ content: coverageCss });
            const chromiumCoverage = `${prefix}-coverage-chromium.png`;
            await page.screenshot({ path: chromiumCoverage });
            const coverage = compareCoverage(
                await readNative(coverageRgba, `${prefix}-coverage-native.png`, width, height),
                readPixels(chromiumCoverage, width, height),
            );
            assert(coverage.after.iou >= 0.85, `${name} ${width}: ::after coverage diverged`);
            if (width === 390 && name === 'walk') {
                assert.equal(coverage.before.native + coverage.before.chromium, 0,
                    'No sampled ::before face should survive in the portrait walk state');
            } else {
                assert(coverage.before.iou >= 0.50, `${name} ${width}: ::before coverage diverged`);
            }
            const motion = previous && {
                native: compare(actual, previous.actual).changedFraction,
                chromium: compare(reference, previous.reference).changedFraction,
            };
            if (motion) assert(motion.native >= 0.2 && motion.chromium >= 0.2,
                `${name} ${width}: pose did not change`);
            previous = { actual, reference };

            const lossesByProperty = Object.create(null);
            for (const { property } of losses) {
                lossesByProperty[property] = (lossesByProperty[property] ?? 0) + 1;
            }
            console.log(JSON.stringify({ width, height, name, radios, gpuiExactRgba: true,
                paintedPlanes: planes.length, fullPage, context, coverage, motion, lossesByProperty }));
        }
    }
});

async function readNative(rgba, native, width, height) {
    execFileSync('xvfb-run', ['-a', '-s', '-screen 0 1600x1000x24', 'bash', capture,
        executable, String(width), String(height), rgba, native], { timeout: 15_000 });
    const expected = await readFile(rgba);
    const actual = readPixels(native, width, height);
    assert(actual.equals(expected), `${native}: GPUI readback differs from the Rust frame`);
    return actual;
}

function readPixels(path, width, height) {
    const dimensions = execFileSync('identify', ['-format', '%w %h', path], { encoding: 'utf8' });
    assert.equal(dimensions, `${width} ${height}`, path);
    return execFileSync('convert', [path, '-alpha', 'on', '-depth', '8', 'rgba:-'], {
        maxBuffer: width * height * 4 + 1024,
    });
}

function compare(native, chromium) {
    let error = 0;
    let changed = 0;
    for (let offset = 0; offset < native.length; offset += 4) {
        const r = Math.abs(native[offset] - chromium[offset]);
        const g = Math.abs(native[offset + 1] - chromium[offset + 1]);
        const b = Math.abs(native[offset + 2] - chromium[offset + 2]);
        error += r + g + b;
        changed += Math.max(r, g, b) >= 8;
    }
    const pixels = native.length / 4;
    return {
        meanAbsoluteRgbError: error / (pixels * 3),
        changedFraction: changed / pixels,
    };
}

function compareCoverage(native, chromium) {
    const counts = Array.from({ length: 2 }, () => ({ native: 0, chromium: 0, intersection: 0 }));
    for (let offset = 0; offset < native.length; offset += 4) {
        for (let channel = 0; channel < 2; channel++) {
            const a = native[offset + channel] >= 128;
            const b = chromium[offset + channel] >= 128;
            counts[channel].native += a;
            counts[channel].chromium += b;
            counts[channel].intersection += a && b;
        }
    }
    return Object.fromEntries(counts.map(({ native, chromium, intersection }, index) => [
        index === 0 ? 'before' : 'after',
        { native, chromium, iou: native || chromium ? intersection / (native + chromium - intersection) : null },
    ]));
}
