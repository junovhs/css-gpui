import assert from 'node:assert/strict';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import { compare, readPixels } from './pixels.mjs';
import { isolateContext, viewports, withBackrooms, withoutViewportLighting } from './probe.mjs';
import { replayDocument } from './route.mjs';

const executable = process.argv[3] && resolve(process.argv[3]);
assert(executable, 'Pass the css-gpui-planar-preview binary as the second argument');
const captures = process.argv[4] && resolve(process.argv[4]);

const steps = [
    { name: 'start' },
    { name: 'enter', target: 'play', selector: 'start-text label[for=play]' },
    { name: 'walk', target: 'z-8', selector: 'buttons label.upb[for=z-8]' },
    { name: 'branch', target: 'z-7', selector: 'buttons label.upb[for=z-7]', doubleClick: true },
    { name: 'return', target: 'z-8', selector: 'buttons label.downb[for=z-8]' },
    { name: 'turn', target: 'right1', selector: 'buttons label.rightb[for=right1]' },
    { name: 'workings', target: 'bh', selector: 'label[for=bh]' },
    { name: 'top', target: 'top-view', selector: '#top-view' },
    { name: 'restore', target: 'top-view', selector: '#top-view' },
];

await withBackrooms(async ({ page, snapshot, reset, htmlPath, cssPath, directory }) => {
    const out = captures ?? directory;
    await mkdir(out, { recursive: true });
    const unlitFile = join(directory, 'without-viewport-lighting.css');
    await writeFile(unlitFile, withoutViewportLighting);

    for (const [width, height] of viewports) {
        await reset(width, height);
        await page.addStyleTag({
            content: '*::before, *::after { transition: none !important; animation: none !important; }',
        });

        const states = [];
        const activations = [];
        for (const { name, target, selector, doubleClick } of steps) {
            if (target) {
                const label = page.locator(selector);
                const control = await label.evaluate(element => ({
                    target: (element.control ?? element).id,
                    class_name: element.className,
                    direct: !element.control,
                }));
                assert.equal(control.target, target);
                const previous = states.at(-1);
                const labels = previous.labels.filter(label => label.target === target &&
                    label.class_name === control.class_name && Boolean(label.direct) === control.direct);
                assert.equal(labels.length, 1, `${width}x${height} ${name}: native control for the browser label`);
                [previous.activation] = labels;
                if (doubleClick) previous.activation.doubleClick = true;
                await label.click({ timeout: 5000 });
                await page.mouse.move(0, 0);
                activations.push(target);
            }
            const browser = await page.evaluate(() => ({
                selection: Object.fromEntries([...document.querySelectorAll('input[type=radio]:checked')]
                    .map(input => [input.name, input.id])),
                checked: [...document.querySelectorAll('input[type=checkbox]:checked')]
                    .map(input => input.id).sort(),
            }));
            const prefix = join(out, `${width}x${height}-live-${name}`);
            const authored = `${prefix}-authored-chromium.png`;
            await page.screenshot({ path: authored });
            const isolated = `${prefix}-chromium.png`;
            const isolation = await isolateContext(page);
            await page.screenshot({ path: isolated });
            await isolation.evaluate(style => style.remove());
            const rgba = `${prefix}.rgba`;
            const { controls } = snapshot('--planar-preview', width, height, activations,
                JSON.stringify({ root: 'content', label_scope: 'body', rgba }));
            const unlitRgba = `${prefix}-isolated.rgba`;
            snapshot('--planar-preview', width, height, activations,
                JSON.stringify({ root: 'content', rgba: unlitRgba }), unlitFile);
            assert.deepEqual(controls.selection, browser.selection, `${name}: Chromium and Rust selection`);
            assert.deepEqual(controls.checked ?? [], browser.checked, `${name}: Chromium and Rust checked IDs`);
            states.push({ name, rgba, authored, isolated, unlitRgba, ...controls });
        }

        // The authored corner bundle contains z-9 and z-8 but no z-7 pose.
        const prefix = await replayDocument(executable, htmlPath, cssPath, states, out, width, height, 'live');
        const native = states.map((_, index) => readPixels(`${prefix}-${index}.png`, width, height));
        const chromium = states.map(({ isolated }) => readPixels(isolated, width, height));
        for (const [index, { name, rgba }] of states.entries()) {
            assert(native[index].equals(await readFile(rgba)), `${name}: exact live native scene readback`);
        }
        const indexOf = name => steps.findIndex(step => step.name === name);
        for (const [name, pixels] of [['native', native], ['Chromium', chromium]]) {
            const at = step => pixels[indexOf(step)];
            assert(compare(at('walk'), at('branch')).changedFraction > 0.10, `${name}: branch changes the scene`);
            assert(at('walk').equals(at('return')), `${name}: returning restores the scene`);
            assert(at('workings').equals(at('restore')), `${name}: toggling Top view off restores the scene`);
        }

        const isolatedRgbError = Object.fromEntries(await Promise.all(states.map(async ({ name, unlitRgba }, index) =>
            [name, compare(await readFile(unlitRgba), chromium[index]).meanAbsoluteRgbError])));
        const fullPageRgbError = Object.fromEntries(states.map(({ name, authored }, index) =>
            [name, compare(native[index], readPixels(authored, width, height)).meanAbsoluteRgbError]));
        assert(isolatedRgbError.branch <= 13, `${width}x${height}: isolated branch scene exceeds the corner error cap`);
        console.log(JSON.stringify({
            width, height, browserClicks: states.length - 1, nativeActivations: states.length - 1,
            gpuiExactRgbaFrames: states.length, fullPageRgbError, isolatedRgbError,
        }));
    }
});
