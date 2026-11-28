import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { isolateContext, viewports, withBackrooms, withoutViewportLighting } from './probe.mjs';
import { prepareCorner } from './route.mjs';
import { compare, readPixels } from './pixels.mjs';

const executable = process.argv[3] && resolve(process.argv[3]);
assert(executable, 'Pass the css-gpui-planar-preview binary as the second argument');
const captures = process.argv[4] && resolve(process.argv[4]);
const capture = fileURLToPath(new URL('./capture-planar.sh', import.meta.url));
const replay = fileURLToPath(new URL('./replay-route.sh', import.meta.url));

const cornerTiles = [6, 7, 8, 9].flatMap(column =>
    [6, 7, 8, 9, 10].map(row => `body content floor > tile.col-${column}.row-${row}`));
const withoutWallImages = `
    body content floor > tile::before, body content floor > tile::after {
        background-image: none !important;
    }
`;
const withoutForegroundWallFilter = `
    body content floor > tile.col-8.row-8::before { filter: none !important; }
`;
const turnWallCrops = new Map([
    [390, [12, 65, 70, 385]],
    [1440, [180, 150, 390, 465]],
]);
const coverageCss = `
    ${withoutViewportLighting}
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
    const unlitFile = join(directory, 'without-viewport-lighting.css');
    const noWallImagesFile = join(directory, 'without-wall-images.css');
    await writeFile(coverageFile, coverageCss);
    await writeFile(unlitFile, withoutViewportLighting);
    await writeFile(noWallImagesFile, withoutWallImages);
    for (const [width, height] of viewports) {
        const { states, bundle } = await prepareCorner(snapshot, out, width, height);
        const nativeCaptures = await replayNative(bundle, states, out, width, height);
        await reset(width, height);
        await page.addStyleTag({
            content: '*::before, *::after { transition: none !important; animation: none !important; }',
        });
        let previous;
        for (const [index, state] of states.entries()) {
            const { name, planes, losses, scope, activation, selection, labels, overrides } = state;
            const prefix = join(out, state.prefix);
            const browser = await browserControls(page, scope);
            assert.deepEqual(browser.selection, selection, `${prefix}: checked radio groups`);
            assert.deepEqual(browser.labels, labels.map(({ group, target, class_name }) =>
                ({ group, target, class_name })), `${prefix}: available labels`);
            if (index === 2) {
                const enter = states[1];
                await clickBrowserLabel(page, scope, backLabel(states));
                assert.deepEqual((await browserControls(page, enter.scope)).selection, enter.selection,
                    `${prefix}: browser backtracking`);
                await clickBrowserLabel(page, enter.scope, enter.activation);
                assert.deepEqual((await browserControls(page, scope)).selection, selection,
                    `${prefix}: browser resumed walk`);
            }
            if (index > 0) {
                const actual = nativeCaptures[index];
                if (captures) await writeFile(`${prefix}-planes.json`, JSON.stringify({ planes, losses }, null, 2));
                const authored = `${prefix}-authored.png`;
                const browserUnlit = `${prefix}-without-overlay-chromium.png`;
                const nativeUnlit = `${prefix}-without-overlay-native.png`;
                const unlitRgba = `${prefix}-without-overlay.rgba`;
                const isolated = `${prefix}-isolated.png`;
                await page.screenshot({ path: authored });
                const authoredPixels = readPixels(authored, width, height);
                let wallCrop;
                if (name === 'turn') {
                    const noWallImagesRgba = `${prefix}-without-wall-images.rgba`;
                    const noWallImagesNative = `${prefix}-without-wall-images-native.png`;
                    const noWallImagesChromium = `${prefix}-without-wall-images-chromium.png`;
                    const noWallFilterChromium = `${prefix}-without-wall-filter-chromium.png`;
                    const filterStyle = await page.addStyleTag({ content: withoutForegroundWallFilter });
                    await page.screenshot({ path: noWallFilterChromium });
                    await filterStyle.evaluate(node => node.remove());
                    const wallStyle = await page.addStyleTag({ content: withoutWallImages });
                    await page.screenshot({ path: noWallImagesChromium });
                    await wallStyle.evaluate(node => node.remove());
                    snapshot('--planar-preview', width, height, overrides,
                        JSON.stringify({ root: 'content', rgba: noWallImagesRgba }), noWallImagesFile);
                    const native = await readNative(noWallImagesRgba, noWallImagesNative, width, height);
                    const browser = readPixels(noWallImagesChromium, width, height);
                    const region = turnWallCrops.get(width);
                    const comparison = compareEffects(width, region, actual, native, authoredPixels, browser);
                    assert(comparison.chromiumChange >= 1 && comparison.nativeChange >= 1,
                        `${prefix}: authored background images must affect the foreground wall crop`);
                    assert(comparison.signedChangeError < 0.55 * comparison.chromiumChange,
                        `${prefix}: wallpaper response must improve on omitting the images`);
                    assert(comparison.authoredError < comparison.omittedError,
                        `${prefix}: paint with wallpaper must improve the authored crop RGB error`);
                    const detail = {
                        chromium: horizontalDetail(width, region, authoredPixels, browser),
                        chromiumFilterOff: horizontalDetail(width, region,
                            readPixels(noWallFilterChromium, width, height), browser),
                        native: horizontalDetail(width, region, actual, native),
                    };
                    // Vertical marks contribute contrast along the horizontal scanline.
                    assert(detail.chromiumFilterOff >= 1.8 * detail.chromium,
                        `${prefix}: disabling the authored filter must sharpen the pattern`);
                    assert(detail.native >= 0.5 * detail.chromium && detail.native <= 1.25 * detail.chromium,
                        `${prefix}: foreground texture contrast differs from Chromium`);
                    assert(!losses.some(({ property, value }) =>
                        property === 'background-image' && value.startsWith('radial-gradient(')),
                    `${prefix}: supported radial wallpaper was reported as a loss`);
                    wallCrop = { bounds: region, ...comparison, horizontalDetail: detail };
                }
                const unlitStyle = await page.addStyleTag({ content: withoutViewportLighting });
                await page.screenshot({ path: browserUnlit });
                await unlitStyle.evaluate(node => node.remove());
                snapshot('--planar-preview', width, height, overrides,
                    JSON.stringify({ root: 'content', rgba: unlitRgba }), unlitFile);
                const unlit = await readNative(unlitRgba, nativeUnlit, width, height);
                const isolation = await isolateContext(page);
                await page.screenshot({ path: isolated });

                const reference = readPixels(isolated, width, height);
                const unlitPixels = readPixels(browserUnlit, width, height);
                const fullPage = compare(actual, authoredPixels);
                const fullPageBefore = compare(unlit, authoredPixels);
                const fullPageWithoutOverlay = compare(unlit, unlitPixels);
                const context = compare(unlit, reference);
                const lighting = lightingRegions(width, height, actual, unlit, authoredPixels, unlitPixels);
                assert(fullPageBefore.meanAbsoluteRgbError - fullPage.meanAbsoluteRgbError >= 25,
                    `${prefix}: viewport overlay did not improve RGB error by at least 25`);
                assert(lighting.every(({ signedChangeError, chromiumChange }) =>
                    signedChangeError <= 15 && chromiumChange >= 15),
                `${prefix}: viewport overlay differs across the viewport ${JSON.stringify(lighting)}`);
                assert(!losses.some(({ property, value }) => property === 'mix-blend-mode' && value === 'overlay'),
                    `${prefix}: authored viewport overlay was reported as a loss`);
                assert(planes.length >= 20, `${name} ${width}: projected too few painted faces`);
                assert(planes.every(({ polygon }) => polygon.every(([x, y]) =>
                    x >= -1e-6 && y >= -1e-6 && x <= width + 1e-6 && y <= height + 1e-6)),
                `${name} ${width}: visible polygon escaped the viewport`);
                assert(context.meanAbsoluteRgbError <= 13,
                    `${name} ${width}: isolated context RGB error ${context.meanAbsoluteRgbError}`);
                const coverageRgba = `${prefix}-coverage.rgba`;
                snapshot('--planar-preview', width, height, overrides,
                    JSON.stringify({ root: 'content', rgba: coverageRgba }), coverageFile);
                const coverageStyle = await page.addStyleTag({ content: coverageCss });
                const chromiumCoverage = `${prefix}-coverage-chromium.png`;
                await page.screenshot({ path: chromiumCoverage });
                const coverage = compareCoverage(
                    await readNative(coverageRgba, `${prefix}-coverage-native.png`, width, height),
                    readPixels(chromiumCoverage, width, height),
                );
                await coverageStyle.evaluate(node => node.remove());
                await isolation.evaluate(node => node.remove());
                assert(coverage.after.iou >= 0.85, `${name} ${width}: ::after coverage diverged`);
                if (width === 390 && name === 'walk') {
                    assert.equal(coverage.before.native + coverage.before.chromium, 0,
                        'No sampled ::before face should survive in the portrait walk state');
                } else {
                    assert(coverage.before.iou >= 0.50, `${name} ${width}: ::before coverage diverged`);
                }
                const motion = previous && {
                    native: compare(unlit, previous.unlit).changedFraction,
                    chromium: compare(reference, previous.reference).changedFraction,
                };
                if (motion) assert(motion.native >= 0.2 && motion.chromium >= 0.2,
                    `${name} ${width}: pose did not change`);
                previous = { unlit, reference };

                const lossesByProperty = Object.create(null);
                for (const { property } of losses) {
                    lossesByProperty[property] = (lossesByProperty[property] ?? 0) + 1;
                }
                console.log(JSON.stringify({ width, height, name, selection, gpuiExactRgba: true,
                    paintedPlanes: planes.length, fullPage, fullPageBefore, fullPageWithoutOverlay,
                    lighting, context, coverage, wallCrop, motion, lossesByProperty }));
            }
            if (activation) {
                const clicked = await clickBrowserLabel(page, scope, activation);
                const next = states[index + 1];
                assert.equal((await browserControls(page, next.scope)).selection[activation.group],
                    activation.target, `${prefix}: browser click did not check its bound radio`);
                if (activation.group === 'z') {
                    assert(await clicked.isHidden(),
                        `${prefix}: old forward label should disappear after its radio is checked`);
                }
            }
        }
    }
});

function backLabel(states) {
    return states[2].labels.find(label =>
        label.group === 'z' && label.target === states[1].selection.z);
}

async function clickBrowserLabel(page, scope, { target, class_name }) {
    const classFilter = class_name ? `[class=${JSON.stringify(class_name)}]` : '';
    const label = page.locator(`${scope} label[for=${JSON.stringify(target)}]${classFilter}`);
    await label.click({ timeout: 5000 });
    await page.mouse.move(0, 0);
    return label;
}

async function browserControls(page, scope) {
    return page.evaluate(scope => {
        const selection = Object.fromEntries([...document.querySelectorAll('input[type=radio]:checked')]
            .map(radio => [radio.name, radio.id]));
        const labels = [...document.querySelectorAll(scope + ' label')].flatMap(label => {
            const target = label.control;
            const style = getComputedStyle(label);
            const rect = label.getBoundingClientRect();
            if (target?.type !== 'radio' || target.disabled || style.visibility !== 'visible' ||
                style.pointerEvents === 'none' || rect.right <= 0 || rect.bottom <= 0 ||
                rect.left >= innerWidth || rect.top >= innerHeight || !rect.width || !rect.height) {
                return [];
            }
            return [{ group: target.name, target: target.id, class_name: label.className }];
        });
        return { selection, labels };
    }, scope);
}

async function replayNative(bundle, states, out, width, height) {
    const prefix = join(out, `${width}x${height}-route`);
    const inputs = [];
    const frame = index => ({ frame: index, selection: states[index].selection });
    const expected = [frame(0)];
    const x = (index, label) => {
        const labels = states[index].labels;
        assert(labels.includes(label), `Missing native replay label in state ${index}`);
        return Math.floor(width * (labels.indexOf(label) + 0.5) / labels.length);
    };
    const activate = (from, to, label = states[from].activation, shot = to, keyboard = false) => {
        inputs.push(`${keyboard ? 'keyboard' : x(from, label)} ${to} ${shot}`);
        expected.push({ activated: label.target, from, to }, frame(to));
    };
    activate(0, 1);
    activate(1, 2);
    // Reclick the old forward slot after its radio binding switches to the
    // next tile, which lies outside the compiled corner.
    inputs.push(`${x(1, states[1].activation)} 2 2-unchanged`);
    activate(2, 1, backLabel(states), '1-back');
    activate(1, 2, states[1].activation, '2-return');
    activate(2, 3, states[2].activation, 3, width === 390);
    execFileSync('xvfb-run', ['-a', '-s', '-screen 0 1600x1000x24', 'bash', replay,
        executable, bundle, String(width), String(height), prefix], {
        input: inputs.join('\n') + '\n', timeout: 15_000,
    });
    const trace = (await readFile(`${prefix}-trace.jsonl`, 'utf8')).trim().split('\n').map(line => JSON.parse(line));
    assert.deepEqual(trace, expected, `${prefix}: native input and checked radio groups`);
    const pixels = await Promise.all(states.map(async (state, index) => {
        const actual = readPixels(`${prefix}-${index}.png`, width, height);
        const expected = await readFile(join(out, state.rgba));
        assert(actual.equals(expected), `${state.prefix}: native scene differs from the selected Rust frame`);
        if (captures && index > 0) await writeFile(join(out, `${state.prefix}-native.png`),
            await readFile(`${prefix}-${index}.png`));
        return actual;
    }));
    const unchanged = readPixels(`${prefix}-2-unchanged.png`, width, height);
    assert(unchanged.equals(pixels[2]), `${prefix}: clicking the rebound slot changed the scene`);
    assert(readPixels(`${prefix}-1-back.png`, width, height).equals(pixels[1]),
        `${prefix}: backtracking did not restore the enter frame`);
    assert(readPixels(`${prefix}-2-return.png`, width, height).equals(pixels[2]),
        `${prefix}: walking again did not restore the walk frame`);
    return pixels;
}

async function readNative(rgba, native, width, height) {
    execFileSync('xvfb-run', ['-a', '-s', '-screen 0 1600x1000x24', 'bash', capture,
        executable, String(width), String(height), rgba, native], { timeout: 15_000 });
    const expected = await readFile(rgba);
    const actual = readPixels(native, width, height);
    assert(actual.equals(expected), `${native}: GPUI readback differs from the Rust frame`);
    return actual;
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

function lightingRegions(width, height, native, nativeUnlit, chromium, chromiumUnlit) {
    const regions = [
        ['upper left', 0.08, 0.08, 0.29, 0.25],
        ['center', 0.35, 0.35, 0.65, 0.65],
        ['lower right', 0.71, 0.70, 0.92, 0.86],
    ];
    return regions.map(([region, x0, y0, x1, y1]) => {
        const bounds = [x0 * width, y0 * height, x1 * width, y1 * height].map(Math.floor);
        return { region, ...compareEffects(width, bounds, native, nativeUnlit, chromium, chromiumUnlit) };
    });
}

function compareEffects(width, [left, top, right, bottom], native, nativeOff, chromium, chromiumOff) {
    let error = 0;
    let chromiumChange = 0;
    let nativeChange = 0;
    let authoredError = 0;
    let disabledError = 0;
    let omittedError = 0;
    for (let y = top; y < bottom; y++) {
        for (let x = left; x < right; x++) {
            for (let index = (y * width + x) * 4, end = index + 3; index < end; index++) {
                const browserDelta = chromium[index] - chromiumOff[index];
                const nativeDelta = native[index] - nativeOff[index];
                error += Math.abs(nativeDelta - browserDelta);
                chromiumChange += Math.abs(browserDelta);
                nativeChange += Math.abs(nativeDelta);
                authoredError += Math.abs(native[index] - chromium[index]);
                disabledError += Math.abs(nativeOff[index] - chromiumOff[index]);
                omittedError += Math.abs(nativeOff[index] - chromium[index]);
            }
        }
    }
    const channels = (right - left) * (bottom - top) * 3;
    return { signedChangeError: error / channels, chromiumChange: chromiumChange / channels,
        nativeChange: nativeChange / channels, authoredError: authoredError / channels,
        disabledError: disabledError / channels, omittedError: omittedError / channels };
}

function horizontalDetail(width, [left, top, right, bottom], authored, noImages) {
    let change = 0;
    for (let y = top; y < bottom; y++) {
        for (let x = left; x + 1 < right; x++) {
            const offset = (y * width + x) * 4;
            for (let channel = 0; channel < 3; channel++) {
                const index = offset + channel;
                change += Math.abs((authored[index + 4] - noImages[index + 4]) -
                    (authored[index] - noImages[index]));
            }
        }
    }
    return change / ((right - left - 1) * (bottom - top) * 3);
}
