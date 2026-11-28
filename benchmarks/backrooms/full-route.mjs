import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { compare, readPixels } from './pixels.mjs';
import { viewports, withBackrooms } from './probe.mjs';
import { routeLabels, writeRouteBundle } from './route.mjs';

const executable = process.argv[3] && resolve(process.argv[3]);
assert(executable, 'Pass the css-gpui-planar-preview binary as the second argument');
const captures = process.argv[4] && resolve(process.argv[4]);
const replay = fileURLToPath(new URL('./replay-route.sh', import.meta.url));
const capture = fileURLToPath(new URL('./capture-planar.sh', import.meta.url));
const navigationLabels = 'start-text label[for], buttons label[for], content label[for]';
const maxStates = 4000;
const wallEdit = `
    body content floor > tile::before, body content floor > tile::after {
        background-color: #f02ad7 !important;
        background-image: none !important;
    }
`;

await withBackrooms(async ({ page, newPage, snapshot, reset, directory }) => {
    const out = captures ?? directory;
    await mkdir(out, { recursive: true });
    const wallFile = join(directory, 'wall-edit.css');
    await writeFile(wallFile, wallEdit);

    const viewportPages = await Promise.all(viewports.map(async ([width, height], index) => {
        const target = index === 0 ? page : await newPage();
        const resetSettled = async () => {
            await reset(width, height, target);
            await target.addStyleTag({
                content: '*::before, *::after { transition: none !important; animation: none !important; }',
            });
        };
        await resetSettled();
        return { width, height, page: target, resetSettled };
    }));
    const { route, search } = await discoverRoute(viewportPages.map(({ page }) => page));
    console.log(JSON.stringify({ search, length: route.length - 1 }));

    for (const { width, height, page, resetSettled } of viewportPages) {
        await resetSettled();
        const browserStates = await replayBrowser(page, out, width, height, route);
        const states = compileRoute(snapshot, out, width, height, route, browserStates);
        const bundle = await writeRouteBundle(out, width, height, states, 'full-route');
        const nativePrefix = await replayNative(bundle, states, out, width, height);
        const frames = await compareFrames(out, width, height, nativePrefix, states, browserStates);
        const wall = await verifyWallEdit(snapshot, out, width, height, nativePrefix, states, browserStates, wallFile);
        console.log(JSON.stringify({
            width, height, browserClicks: route.length - 1, nativeClicks: route.length - 1,
            gpuiExactRgbaFrames: frames.length, wallEdit: wall,
            checkpoints: route.slice(1).flatMap(({ activation, selection }, index) =>
                ['bookends', 'floor'].includes(activation.group)
                    ? [{ step: index + 1, target: activation.target, selection }] : []),
            frames,
        }));
    }
});

function selectionKey(selection) {
    return JSON.stringify(Object.entries(selection).sort(([a], [b]) => a.localeCompare(b)));
}

function sameLabel(a, b) {
    return a.group === b.group && a.target === b.target && a.class_name === b.class_name;
}

async function browserState(page, selection) {
    return page.evaluate(({ selector, selection }) => {
        // Discovery sets a candidate state. Replay supplies no overrides.
        for (const target of Object.values(selection ?? {})) document.getElementById(target).checked = true;
        const checked = Object.fromEntries([...document.querySelectorAll('input[type=radio]:checked')]
            .map(radio => [radio.name, radio.id]));
        const labels = [...document.querySelectorAll(selector)].flatMap(label => {
            const target = label.control;
            if (target?.type !== 'radio' || target.disabled || target.checked) return [];
            // Transformed doors can have a clickable descendant outside the
            // label's rectangle. Sample both and let Chromium decide the hit.
            for (const part of [label, ...label.querySelectorAll('*')]) {
                const rect = part.getBoundingClientRect();
                const left = Math.max(rect.left, 0);
                const right = Math.min(rect.right, innerWidth);
                const top = Math.max(rect.top, 0);
                const bottom = Math.min(rect.bottom, innerHeight);
                if (right <= left || bottom <= top) continue;
                for (const fx of [0.5, 0.25, 0.75, 0.1, 0.9]) {
                    for (const fy of [0.5, 0.25, 0.75, 0.1, 0.9]) {
                        const x = left + (right - left) * fx;
                        const y = top + (bottom - top) * fy;
                        if (document.elementFromPoint(x, y)?.closest('label') === label) {
                            return [{ target: target.id, group: target.name, class_name: label.className, x, y }];
                        }
                    }
                }
            }
            return [];
        });
        return { selection: checked, labels };
    }, { selection, selector: navigationLabels });
}

async function discoverRoute(pages) {
    const [initial, ...otherViewports] = await Promise.all(pages.map(page => browserState(page)));
    for (const { selection } of otherViewports) {
        assert.deepEqual(selection, initial.selection, 'Authored initial radios must match in both viewports');
    }
    const queue = [{ selection: initial.selection }];
    const seen = new Set([selectionKey(initial.selection)]);
    for (let index = 0; index < queue.length; index++) {
        const current = queue[index];
        if (current.selection.bookends === 'finish') {
            const route = [];
            for (let state = current; state; state = state.previous) {
                const { selection, activation } = state;
                route.push({ selection, activation });
            }
            return { route: route.reverse(), search: { examined: index, discovered: queue.length } };
        }
        const [primary, ...others] = await Promise.all(pages.map(page => browserState(page, current.selection)));
        const labels = primary.labels.filter(label =>
            others.every(({ labels }) => labels.some(candidate => sameLabel(label, candidate))));
        for (const { target, group, class_name } of labels) {
            const selection = { ...current.selection, [group]: target };
            const key = selectionKey(selection);
            if (seen.has(key)) continue;
            assert(seen.size < maxStates, `Route discovery exceeded ${maxStates} checked states`);
            seen.add(key);
            queue.push({ selection, activation: { target, group, class_name }, previous: current });
        }
    }
    assert.fail(`Finish was unreachable through sampled labels after ${queue.length} checked states`);
}

async function replayBrowser(page, out, width, height, route) {
    assert.equal(route[0].selection.bookends, 'start');
    assert.equal(route.at(-1).selection.bookends, 'finish');
    assert(route.some(({ activation }) => activation?.target === 'level-2'), 'Route must reach level two');
    const states = [];
    for (const [index, { activation, selection }] of route.entries()) {
        const label = index && states[index - 1].labels.find(label => sameLabel(label, activation));
        if (index) {
            assert(label, `${width}x${height} step ${index}: no browser hit for ${activation.target}`);
            await page.mouse.click(label.x, label.y);
            await page.mouse.move(0, 0);
        }
        const actual = await browserState(page);
        assert.deepEqual(actual.selection, selection, `${width}x${height} step ${index}: Browser click selection`);
        const png = join(out, `${width}x${height}-full-${index}-chromium.png`);
        await page.screenshot({ path: png });
        const state = { ...actual, png };
        if (activation?.target === 'level-2') {
            const edit = await page.addStyleTag({ content: wallEdit });
            state.editedPng = join(out, `${width}x${height}-full-${index}-wall-chromium.png`);
            await page.screenshot({ path: state.editedPng });
            await edit.evaluate(style => style.remove());
        }
        states.push(state);
    }
    return states;
}

function compileRoute(snapshot, out, width, height, route, browserStates) {
    const overrides = [];
    return route.map(({ selection }, index) => {
        const rgba = `${width}x${height}-full-${index}.rgba`;
        const prefix = `${width}x${height} step ${index}`;
        const { controls, planes, losses } = snapshot('--planar-preview', width, height, overrides,
            JSON.stringify({ root: 'content', label_scope: 'body', rgba: join(out, rgba) }));
        assert.deepEqual(controls.selection, selection, `${prefix}: Rust selection`);
        for (const label of browserStates[index].labels) {
            assert.equal(controls.labels.filter(candidate => sameLabel(candidate, label)).length, 1,
                `${prefix}: Rust control for the browser's ${label.target} (${label.class_name}) label`);
        }
        const labels = routeLabels(controls.labels);
        const next = route[index + 1]?.activation;
        const activation = next && labels.find(label => sameLabel(label, next));
        if (next) {
            assert(activation, `${prefix}: missing Rust control for ${next.target}`);
            overrides.push(activation.target);
        }
        return { rgba, selection: controls.selection, labels, planes: planes.length, losses, activation };
    });
}

async function replayNative(bundle, states, out, width, height) {
    const prefix = join(out, `${width}x${height}-full-native`);
    const frame = index => ({ frame: index, selection: states[index].selection });
    const expected = [frame(0)];
    const inputs = states.slice(0, -1).map(({ activation, labels }, index) => {
        const slot = labels.findIndex(label => sameLabel(label, activation));
        assert(slot >= 0, `${prefix}: missing native control for ${activation.target}`);
        const x = Math.floor(width * (slot + 0.5) / labels.length);
        expected.push({ activated: activation.target, from: index, to: index + 1 }, frame(index + 1));
        return `${x} ${index + 1} ${index + 1}`;
    });
    execFileSync('xvfb-run', ['-a', '-s', '-screen 0 1600x1000x24', 'bash', replay,
        executable, bundle, String(width), String(height), prefix], {
        input: inputs.join('\n') + '\n', timeout: 15_000 + 1500 * inputs.length,
    });
    const trace = (await readFile(`${prefix}-trace.jsonl`, 'utf8')).trim().split('\n').map(line => JSON.parse(line));
    assert.deepEqual(trace, expected, `${prefix}: native X11 input trace and checked selection`);
    return prefix;
}

async function compareFrames(out, width, height, nativePrefix, states, browserStates) {
    const frames = [];
    for (const [index, { rgba, selection, planes, losses }] of states.entries()) {
        const native = readPixels(`${nativePrefix}-${index}.png`, width, height);
        assert(native.equals(await readFile(join(out, rgba))), `${rgba}: native readback differs from Rust frame`);
        const browser = readPixels(browserStates[index].png, width, height);
        const lossesByProperty = Object.create(null);
        for (const { property } of losses) lossesByProperty[property] = (lossesByProperty[property] ?? 0) + 1;
        frames.push({ index, selection, planes, fullPage: compare(native, browser), lossesByProperty });
    }
    if (captures) {
        await writeFile(`${nativePrefix}-comparison.json`, JSON.stringify(frames, null, 2));
    }
    return frames;
}

async function verifyWallEdit(snapshot, out, width, height, nativePrefix, states, browserStates, wallFile) {
    const index = browserStates.findIndex(state => state.editedPng);
    assert(index > 0, 'The wall edit requires a recorded frame after switching floors');
    const { png, editedPng } = browserStates[index];
    const rgba = join(out, `${width}x${height}-full-${index}-wall.rgba`);
    const editedNative = `${nativePrefix}-${index}-wall.png`;
    snapshot('--planar-preview', width, height, Object.values(states[index].selection),
        JSON.stringify({ root: 'content', rgba }), wallFile);
    execFileSync('xvfb-run', ['-a', '-s', '-screen 0 1600x1000x24', 'bash', capture,
        executable, String(width), String(height), rgba, editedNative], { timeout: 15_000 });
    const editedPixels = readPixels(editedNative, width, height);
    assert(editedPixels.equals(await readFile(rgba)), `${rgba}: GPUI wall edit differs from the rebuilt frame`);
    const native = compare(readPixels(`${nativePrefix}-${index}.png`, width, height), editedPixels);
    const chromium = compare(readPixels(png, width, height), readPixels(editedPng, width, height));
    assert(native.changedFraction >= 0.01 && chromium.changedFraction >= 0.01,
        `${rgba}: wall edit changed fewer than 1% of pixels in Chromium or GPUI`);
    return { step: index, native, chromium };
}
