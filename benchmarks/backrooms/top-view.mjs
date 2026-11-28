import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { measure, measureUsedBoxes } from './geometry.mjs';
import { compare, readPixels } from './pixels.mjs';
import { isolateContext, viewports, withBackrooms, withoutViewportLighting } from './probe.mjs';
import { routeLabels, writeRouteBundle } from './route.mjs';

const executable = process.argv[3] && resolve(process.argv[3]);
assert(executable, 'Pass the css-gpui-planar-preview binary as the second argument');
const captures = process.argv[4] && resolve(process.argv[4]);
const replay = fileURLToPath(new URL('./replay-route.sh', import.meta.url));
const steps = [
    { name: 'start' },
    { name: 'enter', target: 'play', selector: 'start-text label[for=play]' },
    { name: 'workings', target: 'bh', selector: 'bh label[for=bh]' },
    { name: 'top', target: 'top-view', selector: '#top-view', direct: true },
    { name: 'restore', target: 'top-view', selector: '#top-view', direct: true },
];

await withBackrooms(async ({ page, snapshot, reset, directory }) => {
    const out = captures ?? directory;
    await mkdir(out, { recursive: true });
    const unlitFile = join(directory, 'without-viewport-lighting.css');
    await writeFile(unlitFile, withoutViewportLighting);
    const cdp = await page.context().newCDPSession(page);
    await cdp.send('DOM.enable');
    for (const [width, height] of viewports) {
        await reset(width, height);
        await page.addStyleTag({
            content: '*::before, *::after { transition: none !important; animation: none !important; }',
        });
        const screen = `${width}x${height}`;
        const states = [];
        const checkpoints = [];
        const activations = [];
        for (const { name, target, selector, direct = false } of steps) {
            if (target) {
                const locator = page.locator(selector);
                const actualTarget = await locator.evaluate(element => (element.control ?? element).id);
                assert.equal(actualTarget, target, `${screen} ${name}: authored activation target`);
                await locator.click({ timeout: 5000 });
                await page.mouse.move(0, 0);
                activations.push(target);
            }
            const chromium = await browserState(page);
            const png = join(out, `${screen}-top-view-${name}-chromium.png`);
            await page.screenshot({ path: png });
            const rgba = `${screen}-top-view-${name}.rgba`;
            const { controls, losses, planes } = snapshot('--planar-preview', width, height, activations,
                JSON.stringify({ root: 'content', label_scope: 'body', rgba: join(out, rgba) }));
            assert.deepEqual(controls.selection, chromium.selection, `${screen} ${name}: radio selection`);
            assert.deepEqual(controls.checked ?? [], chromium.checked, `${screen} ${name}: checked IDs`);
            const labels = routeLabels(controls.labels);
            for (const [id, visible] of Object.entries(chromium.checkboxes)) {
                assert.equal(labels.some(label => label.direct && label.target === id), visible,
                    `${screen} ${name}: direct checkbox visibility for ${id}`);
            }

            const check = { name, png, planes: planes.length, losses: tallyLosses(losses), camera: chromium.camera };
            if (name === 'top') {
                check.geometry = await compareTopGeometry(page, cdp, snapshot, width, height, activations);
                const isolation = await isolateContext(page);
                try {
                    check.isolated = join(out, `${screen}-top-view-isolated-chromium.png`);
                    await page.screenshot({ path: check.isolated });
                } finally {
                    await isolation.evaluate(style => style.remove());
                }
                check.isolatedRgba = join(out, `${screen}-top-view-isolated.rgba`);
                snapshot('--planar-preview', width, height, activations,
                    JSON.stringify({ root: 'content', rgba: check.isolatedRgba }), unlitFile);
            }
            // The second Top view click must select the existing workings frame.
            const frame = name === 'restore' ? 2 : states.length;
            if (name === 'restore') {
                const previous = states[frame];
                assert.deepEqual(controls.checked, previous.checked);
                assert.deepEqual(controls.selection, previous.selection);
                assert.deepEqual(labels, previous.labels);
                assert((await readFile(join(out, rgba))).equals(await readFile(join(out, previous.rgba))),
                    `${screen}: Rust restored the previous scene`);
            } else {
                states.push({ rgba, selection: controls.selection, checked: controls.checked, labels });
            }
            if (target) {
                const from = checkpoints.at(-1).frame;
                const slot = states[from].labels.findIndex(label =>
                    label.target === target && Boolean(label.direct) === direct);
                assert(slot >= 0, `${screen} ${name}: missing GPUI control for ${target}`);
                const x = Math.floor(width * (slot + 0.5) / states[from].labels.length);
                check.input = { target, from, x };
            }
            checkpoints.push({ ...check, frame });
        }
        assert.deepEqual(checkpoints.map(({ frame }) => states[frame].checked ?? []),
            [[], [], ['bh'], ['bh', 'top-view'], ['bh']]);
        assert.notEqual(checkpoints[2].camera, checkpoints[3].camera);
        assert.equal(checkpoints[2].camera, checkpoints[4].camera);

        const bundle = await writeRouteBundle(out, width, height, states, 'top-view');
        const frames = await replayNative(bundle, states, checkpoints, out, width, height);
        console.log(JSON.stringify({ width, height, browserClicks: 4, nativeClicks: 4, gpuiExactRgbaFrames: 5, frames }));
    }
});

async function browserState(page) {
    return page.evaluate(() => ({
        selection: Object.fromEntries([...document.querySelectorAll('input[type=radio]:checked')]
            .map(radio => [radio.name, radio.id])),
        checked: [...document.querySelectorAll('input[type=checkbox]:checked')].map(input => input.id).sort(),
        checkboxes: Object.fromEntries([...document.querySelectorAll('input[type=checkbox]')].map(input => [
            input.id, Boolean(input.getClientRects().length && getComputedStyle(input).visibility === 'visible'),
        ])),
        camera: getComputedStyle(document.querySelector('content w')).transform,
    }));
}

async function compareTopGeometry(page, cdp, snapshot, width, height, activations) {
    const candidates = ['content floor', ...[5, 6, 7, 8, 9].flatMap(column =>
        [5, 6, 7, 8, 9].map(row => `content floor > tile.col-${column}.row-${row}`))];
    const { root } = await cdp.send('DOM.getDocument');
    const { boxes, chains, reference, walls } = await measure(cdp, root.nodeId, candidates);
    const browserBoxes = await measureUsedBoxes(page, cdp, boxes);
    const { bounds, projected } = snapshot('--projection-snapshot', width, height, activations,
        JSON.stringify({ chains }));
    assert.deepEqual(Object.keys(bounds).sort(), Object.keys(browserBoxes).sort());
    assert.deepEqual(Object.keys(projected).sort(), [...reference.keys()].sort());
    let maxBoxError = 0;
    for (const [path, { origin, size }] of Object.entries(bounds)) {
        maxBoxError = Math.max(maxBoxError, error(origin, browserBoxes[path].origin),
            error(size, browserBoxes[path].size));
    }
    let vertices = 0;
    let wallVertices = 0;
    let maxVertexError = 0;
    for (const [name, { corners }] of Object.entries(projected)) {
        for (const [index, corner] of corners.entries()) {
            if (!corner) continue;
            vertices++;
            if (walls.has(name)) wallVertices++;
            maxVertexError = Math.max(maxVertexError, error(corner, reference.get(name)[index]));
        }
    }
    assert(vertices >= 200, 'Top view comparison requires at least 200 front-of-eye vertices');
    assert(maxBoxError <= 1 / 64, `Top view used box error: ${maxBoxError}px`);
    assert(maxVertexError <= 0.035, `Top view vertex error: ${maxVertexError}px`);
    assert(wallVertices >= 8, 'Top view comparison requires at least eight generated wall vertices');
    return { boxes: boxes.size, vertices, wallVertices, maxBoxError, maxVertexError };
}

function error(actual, expected) {
    return Math.hypot(...actual.map((value, axis) => value - expected[axis]));
}

function tallyLosses(losses) {
    const counts = Object.create(null);
    for (const { property } of losses) counts[property] = (counts[property] ?? 0) + 1;
    return counts;
}

async function replayNative(bundle, states, checkpoints, out, width, height) {
    const prefix = join(out, `${width}x${height}-top-view-native`);
    const frameRecord = frame => {
        const { selection, checked } = states[frame];
        return checked ? { frame, selection, checked } : { frame, selection };
    };
    const expected = [frameRecord(0)];
    const inputs = checkpoints.slice(1).map(({ frame, input: { target, from, x } }, index) => {
        expected.push({ activated: target, from, to: frame }, frameRecord(frame));
        return `${x} ${frame} ${index + 1}`;
    });
    execFileSync('xvfb-run', ['-a', '-s', '-screen 0 1600x1000x24', 'bash', replay,
        executable, bundle, String(width), String(height), prefix], {
        input: inputs.join('\n') + '\n', timeout: 20_000,
    });
    const trace = (await readFile(`${prefix}-trace.jsonl`, 'utf8')).trim().split('\n').map(line => JSON.parse(line));
    assert.deepEqual(trace, expected, `${prefix}: dispatched X11 inputs and checked states`);

    const frames = [];
    const pixels = [];
    for (const [index, { name, frame, png, planes, losses, camera, geometry, isolated, isolatedRgba }] of checkpoints.entries()) {
        const native = readPixels(`${prefix}-${index}.png`, width, height);
        const chromium = readPixels(png, width, height);
        assert(native.equals(await readFile(join(out, states[frame].rgba))), `${name}: exact native readback`);
        const fullPage = compare(native, chromium);
        const context = isolated && compare(await readFile(isolatedRgba), readPixels(isolated, width, height));
        if (context) {
            assert(fullPage.meanAbsoluteRgbError <= 30,
                `${width}x${height}: top view full-page RGB error ${fullPage.meanAbsoluteRgbError}`);
            assert(context.meanAbsoluteRgbError <= 6,
                `${width}x${height}: top view isolated RGB error ${context.meanAbsoluteRgbError}`);
        }
        frames.push({ name, checked: states[frame].checked ?? [], planes, camera, geometry,
            fullPage, context, losses });
        pixels.push({ native, chromium });
    }
    for (const platform of ['native', 'chromium']) {
        const toggle = compare(pixels[2][platform], pixels[3][platform]);
        assert(toggle.changedFraction >= 0.1, `${platform}: overhead view must change at least 10% of pixels`);
        assert(pixels[2][platform].equals(pixels[4][platform]), `${platform}: toggle off restores the source frame`);
        frames[3][`${platform}Toggle`] = toggle;
    }
    return frames;
}
