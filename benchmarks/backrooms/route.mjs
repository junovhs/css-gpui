import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { viewports, withBackroomsSource } from './probe.mjs';

// The bounded route names the authored controls to visit, independent of
// which radio the movement cascade binds to each one.
const corner = [
    { name: 'start', scope: 'start-text', choose: label => label.target === 'play' },
    { name: 'enter', scope: 'buttons', choose: hasClass('upb') },
    { name: 'walk', scope: 'buttons', choose: hasClass('rightb') },
    { name: 'turn', scope: 'buttons' },
];

export async function prepareCorner(snapshot, out, width, height) {
    const overrides = [];
    const states = [];
    for (const { name, scope, choose } of corner) {
        const prefix = `${width}x${height}-${name}`;
        const rgba = `${prefix}.rgba`;
        const { planes, losses, controls } = snapshot(
            '--planar-preview', width, height, overrides,
            JSON.stringify({ root: 'content', label_scope: scope, rgba: join(out, rgba) }),
        );
        const labels = routeLabels(controls.labels);
        const state = { name, scope, prefix, rgba, selection: controls.selection, checked: controls.checked, labels,
            planes, losses, overrides: [...overrides] };
        if (choose) {
            const matches = labels.filter(choose);
            assert.equal(matches.length, 1, `${prefix}: route requires one available authored label`);
            state.activation = matches[0];
            overrides.push(state.activation.target);
        }
        states.push(state);
    }
    const bundle = await writeRouteBundle(out, width, height, states);
    return { states, bundle };
}

export async function writeRouteBundle(out, width, height, states, name = 'route') {
    const bundle = join(out, `${width}x${height}-${name}.json`);
    await writeFile(bundle, JSON.stringify({ width, height, states: states.map(({ rgba, selection, checked, labels }) =>
        ({ rgba, selection, checked, labels })) }, null, 2));
    return bundle;
}

// Send native input through X11. A sequence of snapshots is used solely as
// the oracle for expected controls and pixel readbacks, never by the viewer.
export async function replayDocument(executable, html, css, states, out, width, height, name, browserStates) {
    const prefix = join(out, `${width}x${height}-${name}-native`);
    const replay = fileURLToPath(new URL('./replay-route.sh', import.meta.url));
    const expected = [];
    const inputs = [];
    for (const [index, { selection, checked, labels, activation }] of states.entries()) {
        if (index > 0) {
            expected.push({ activated: states[index - 1].activation.target, from: index - 1, to: index });
        }
        expected.push({
            frame: index, selection, ...(checked?.length ? { checked } : {}),
            labels: labels.map(({ group, target, class_name, checkbox }) =>
                ({ group, target, class_name, checkbox: Boolean(checkbox) })),
        });
        if (activation) {
            const reference = browserStates?.[index].labels;
            const point = reference?.find(label => label.movement && sameControl(label, activation));
            if (point) {
                inputs.push(`${Math.floor(point.x)},${Math.floor(point.y)} ${index + 1} ${index + 1}`);
                continue;
            }
            const stripLabels = browserStates && index === 0
                ? labels.filter(label => !isMovementLabel(label))
                : labels.filter(label => !reference?.some(candidate => candidate.movement && sameControl(label, candidate)));
            const slot = stripLabels.indexOf(activation);
            assert(slot >= 0, `${prefix} frame ${index}: missing native activation`);
            const x = Math.floor(width * (slot + 0.5) / stripLabels.length);
            const input = activation.doubleClick ? `double-${x}` : x;
            inputs.push(`${input} ${index + 1} ${index + 1}`);
        }
    }
    execFileSync('xvfb-run', ['-a', '-s', '-screen 0 1600x1000x24', 'bash', replay,
        executable, browserStates ? '--document' : '--document-strip',
        String(width), String(height), prefix, html, css, 'content', 'body'], {
        input: inputs.join('\n') + '\n', timeout: 20_000 + 4000 * inputs.length,
    });
    const trace = (await readFile(`${prefix}-trace.jsonl`, 'utf8')).trim().split('\n').map(line => JSON.parse(line));
    for (const record of trace) {
        const reference = browserStates?.[record.frame]?.labels;
        for (const label of record.labels ?? []) {
            if (label.movement) {
                const [left, top, w, h] = label.movement.bounds;
                const visible = left < width && top < height && left + w > 0 && top + h > 0;
                const browser = reference?.find(candidate => candidate.movement && sameControl(label, candidate));
                assert.equal(visible, Boolean(browser), `${prefix} frame ${record.frame}: movement visibility for ${label.target}`);
                if (browser) {
                    assert(label.movement.bounds.every((value, edge) => Math.abs(value - browser.movement[edge]) <= 1 / 64),
                        `${prefix} frame ${record.frame}: ${label.target} native ${label.movement.bounds} vs Chromium ${browser.movement}`);
                    assert.equal(label.movement.glyph, browser.arrow, 'authored generated arrow');
                }
            } else {
                assert(!reference?.some(candidate => candidate.movement && sameControl(label, candidate)),
                    `${prefix} frame ${record.frame}: missing authored surface for ${label.target}`);
            }
            delete label.caption;
            delete label.movement;
        }
    }
    assert.deepEqual(trace, expected, `${prefix}: live X11 input trace, available controls, and checked state`);
    return prefix;
}

function sameControl(a, b) {
    return a.group === b.group && a.target === b.target && a.class_name === b.class_name;
}

function isMovementLabel(label) {
    return /^(upb|downb|leftb|rightb)\b/.test(label.class_name);
}

export function routeLabels(labels) {
    return labels.map(label => ({ ...label, caption: caption(label) }));
}

function hasClass(name) {
    return label => label.class_name.split(/\s+/).includes(name);
}

function caption({ text, generated_content, target }) {
    if (text) return text;
    const hex = /^"\\([a-f0-9]{1,6})\s?"$/i.exec(generated_content ?? '');
    if (hex) return String.fromCodePoint(parseInt(hex[1], 16));
    return target;
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
    const output = process.argv[2];
    assert(output && !process.argv[3], 'Usage: node benchmarks/backrooms/route.mjs OUTPUT_DIRECTORY');
    const out = resolve(output);
    await mkdir(out, { recursive: true });
    await withBackroomsSource(async ({ snapshot }) => {
        for (const [width, height] of viewports) {
            const { bundle } = await prepareCorner(snapshot, out, width, height);
            console.log(bundle);
        }
    });
}
