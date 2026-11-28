import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { viewports, withBackrooms } from './probe.mjs';
import { prepareCorner } from './route.mjs';

const executable = process.argv[3] && resolve(process.argv[3]);
assert(executable, 'Pass the css-gpui-planar-preview binary as the second argument');
const captures = process.argv[4] && resolve(process.argv[4]);
const capture = fileURLToPath(new URL('./capture-planar.sh', import.meta.url));
const replay = fileURLToPath(new URL('./replay-route.sh', import.meta.url));

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
                const isolated = `${prefix}-isolated.png`;
                await page.screenshot({ path: authored });
                // Hide siblings of the context and paint from its ancestor
                // pseudo-elements. Keep the whole context, including its own
                // backgrounds, textures, filters and shadows, in the comparison.
                const isolation = await page.addStyleTag({
                    content: `html::before, html::after, body::before, body::after,
                        body > div::before, body > div::after,
                        body > div > div::before, body > div > div::after,
                        body :not(:has(content), content, content *) { visibility: hidden !important; }`,
                });
                await page.screenshot({ path: isolated });

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
                console.log(JSON.stringify({ width, height, name, selection, gpuiExactRgba: true,
                    paintedPlanes: planes.length, fullPage, context, coverage, motion, lossesByProperty }));
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
