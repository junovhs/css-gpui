import assert from 'node:assert/strict';
import { mkdir, writeFile } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
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
