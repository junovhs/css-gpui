// Run under Xvfb to compare browser border boxes to painted GPUI bounds.
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdir, writeFile } from 'node:fs/promises';
import { createRequire } from 'node:module';
import { join, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

export async function comparePositionedBoxes(fixture, boxes) {
    const [playwright, output] = process.argv.slice(2).map(path => resolve(path));
    assert(playwright && output, 'Pass the Playwright installation directory and a new output directory');
    const require = createRequire(join(playwright, 'package.json'));
    assert.equal(require('playwright/package.json').version, '1.60.0');
    const { chromium } = require('playwright');
    const benchmark = fileURLToPath(new URL('./', import.meta.url));
    const root = resolve(benchmark, '../..');
    const probe = join(benchmark, 'probes', fixture);
    const run = (program, args, options = {}) => execFileSync(program, args, {
        cwd: root, encoding: 'utf8', timeout: 60_000, ...options,
    });

    const measure = String.raw`
import json, runpy, sys
from pathlib import Path
from PIL import Image, ImageChops

capture_script, executable, folder, width, height, boxes = sys.argv[1:]
directory = Path(folder)
size = (int(width), int(height))
runpy.run_path(capture_script)['capture'](executable, *size, directory)
with Image.open(directory / 'native-baseline.png') as frame:
    assert frame.size == size
    frame = frame.convert('RGB')
    result = []
    for box in json.loads(boxes):
        color = box['color']
        difference = ImageChops.difference(frame, Image.new('RGB', size, color))
        red, green, blue = difference.split()
        mask = ImageChops.lighter(ImageChops.lighter(red, green), blue).point(lambda x: 255 if x == 0 else 0)
        left, top, right, bottom = mask.getbbox()
        if not box.get('background'):
            assert mask.histogram()[255] == (right - left) * (bottom - top)
        result.append([left, top, right - left, bottom - top])
    print(json.dumps(result))
`;

    await mkdir(output);
    const browser = await chromium.launch();
    try {
        for (const [width, height] of [[400, 600], [500, 700]]) {
            const viewport = `${width}x${height}`;
            const directory = join(output, viewport);
            await mkdir(directory);
            const page = await browser.newPage({ viewport: { width, height } });
            await page.goto(pathToFileURL(join(probe, 'probe.html')).href);
            const reference = await Promise.all(boxes.map(({ selector }) => page.locator(selector).evaluate(box => {
                const { x, y, width, height } = box.getBoundingClientRect();
                return [x, y, width, height];
            })));
            await page.screenshot({ path: join(directory, 'browser.png') });
            await page.close();

            const generated = join(directory, 'generated.rs');
            await writeFile(generated, run('cargo', [
                'run', '--quiet', '--locked', '--example', 'generate_gpui', '--', '--viewport',
                viewport, join(probe, 'probe.html'), join(probe, 'probe.css'),
            ]));
            const build = run('cargo', [
                'build', '--locked', '--manifest-path', join(benchmark, 'native/Cargo.toml'),
                '--message-format=json-render-diagnostics',
            ], { env: { ...process.env, CSS_GPUI_GENERATED: generated }, timeout: 600_000, maxBuffer: 16 * 1024 ** 2 });
            const executable = build.trim().split('\n').map(JSON.parse)
                .find(message => message.target?.name === 'css-gpui-compat-host' && message.executable).executable;
            const native = JSON.parse(run('python3', [
                '-c', measure, join(benchmark, 'capture-native.py'), executable, directory, width, height, JSON.stringify(boxes),
            ]));

            const bounds = { chromium: reference, gpui: native };
            await writeFile(join(directory, 'bounds.json'), JSON.stringify(bounds, null, 2) + '\n');
            for (const [index, box] of native.entries()) {
                assert(box.every((coordinate, axis) => Math.abs(coordinate - reference[index][axis]) < 1),
                    `${viewport} ${boxes[index].selector}: ${JSON.stringify(bounds)}`);
            }
            console.log(`${viewport}: ${JSON.stringify(bounds)}`);
        }
    } finally {
        await browser.close();
    }
}
