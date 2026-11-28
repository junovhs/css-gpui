import assert from 'node:assert/strict';
import { execFileSync, spawn } from 'node:child_process';
import { mkdtemp, readFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { createInterface } from 'node:readline';
import { setTimeout } from 'node:timers/promises';
import { fileURLToPath } from 'node:url';
import { readPixels } from './pixels.mjs';
import { viewports, withBackrooms } from './probe.mjs';

const executable = process.argv[3] && resolve(process.argv[3]);
assert(executable, 'Pass the css-gpui-planar-preview binary as the second argument');
const run = (program, args) => execFileSync(program, args, { encoding: 'utf8', timeout: 12_000 }).trim();
const source = fileURLToPath(new URL('./motion-x11.c', import.meta.url));
const steps = [
    { name: 'walk', target: 'z-8', selector: 'buttons label.upb[for=z-8]', tag: 'Z' },
    { name: 'turn', target: 'right1', selector: 'buttons label.rightb[for=right1]', tag: 'X' },
];

function xFor(labels, width, target) {
    const index = labels.findIndex(label => label.target === target);
    assert(index >= 0, `missing native label ${target}`);
    return Math.floor(width * (index + 0.5) / labels.length);
}

function score(samples, name) {
    const rows = samples.trim().split('\n');
    const [poll, meanPoll, maximumPoll] = rows.pop().split(' ');
    assert.equal(poll, 'poll');
    const times = rows.map(line => Number(line.split(' ')[0])).slice(1);
    assert(times.length >= 10 && times.filter(time => time < 800).length >= 8,
        `${name}: need intermediate frames in the X11 scene, got ${times}`);
    assert(times[0] < 350 && times.at(-1) < 1000,
        `${name}: slow first or final visible change ${times}`);
    const gaps = times.slice(1).map((time, i) => time - times[i]).sort((a, b) => a - b);
    return {
        changes: times.length, firstMs: times[0], finalMs: times.at(-1),
        medianGapMs: Number(gaps[Math.floor(gaps.length / 2)].toFixed(2)),
        largestGapMs: Number(gaps.at(-1).toFixed(2)),
        meanPollMs: Number(meanPoll), maximumPollMs: Number(maximumPoll),
    };
}

async function testNative({ width, height, html, css, sample, out, states, interrupt }) {
    const app = spawn(executable, ['--document', `${width}x${height}`, html, css, 'content', 'body'], {
        env: { ...process.env, CSS_GPUI_TRACE_MOTION: '1' }, stdio: ['ignore', 'pipe', 'inherit'],
    });
    const trace = [];
    let failure;
    app.on('error', error => { failure = error; });
    createInterface({ input: app.stdout }).on('line', line => trace.push(JSON.parse(line)));
    async function settled(frame) {
        for (let attempts = 0; attempts < 250; attempts++) {
            if (failure) throw failure;
            const record = trace.find(record => record.frame === frame);
            if (record) return record;
            assert.equal(app.exitCode, null, 'native window exited early');
            await setTimeout(20);
        }
        assert.fail(`native frame ${frame} did not settle`);
    }

    async function exactEndpoint(window, state) {
        const png = join(out, 'x11-endpoint.png');
        run('import', ['-window', window, '-crop', `${width}x${height}+0+0`, '+repage', png]);
        assert(readPixels(png, width, height).equals(await readFile(state.rgba)),
            `${width}x${height}: incorrect full-resolution native endpoint`);
    }

    try {
        const first = await settled(0);
        const window = run('xdotool', ['search', '--sync', '--onlyvisible', '--pid', String(app.pid)])
            .split('\n')[0];
        run('xdotool', ['windowfocus', window]);
        run('xdotool', ['mousemove', '--window', window,
            String(xFor(first.labels, width, 'play')), String(height + 24), 'click', '1']);
        let current = await settled(1);
        await setTimeout(120);
        const readMotion = (target, duration, extra = []) => run(sample, [
            window, String(width), String(height), String(xFor(current.labels, width, target)),
            String(duration), ...extra.map(String),
        ]);
        if (interrupt) {
            // The right label stays available after the walk is resolved.
            const right = xFor(states[0].labels, width, 'right1');
            const samples = readMotion('z-8', 1350, [right, 300]);
            const endpoint = await settled(3);
            assert.deepEqual(endpoint.selection, states[1].selection);
            assert.deepEqual(trace.flatMap(record => record.activated ?? []), ['play', 'z-8', 'right1']);
            assert(!trace.some(record => record.frame === 2), 'interrupted walk attempted to settle');
            assert(trace.some(record => record.motion === 2) && trace.some(record => record.motion === 3),
                'both movements must visibly start when the turn interrupts the walk');
            assert(samples.split('\n').length >= 16, 'the interrupted route lost visible motion');
            await exactEndpoint(window, states[1]);
            return;
        }
        const evidence = [];
        for (const [index, { target, name }] of steps.entries()) {
            const readback = score(readMotion(target, 1120), `${width}x${height} ${name}`);
            current = await settled(index + 2);
            assert.deepEqual(current.selection, states[index].selection);
            await exactEndpoint(window, states[index]);
            const distinct = new Map();
            for (const record of trace.filter(record => record.motion === index + 2)) {
                if (!distinct.has(record.sample_ms)) distinct.set(record.sample_ms, record);
            }
            const poses = [...distinct.values()];
            assert(poses.length >= 8, 'missing sampled native camera poses');
            evidence.push({ readback, poses });
        }
        return evidence;
    } finally {
        app.kill();
    }
}

async function compareChromium(page, step, poses) {
    await page.locator(step.selector).click();
    const { browser, rootFontSize } = await page.evaluate(async ({ tag, times }) => {
        const transition = document.getAnimations().find(animation => animation instanceof CSSTransition
            && animation.transitionProperty === 'transform' && animation.effect.target.tagName === tag);
        if (!transition) throw new Error(`missing authored transition on ${tag}`);
        transition.pause();
        const duration = transition.effect.getTiming().duration;
        const browser = times.map(at => {
            transition.currentTime = at;
            const matrix = new DOMMatrix(getComputedStyle(transition.effect.target).transform);
            return tag === 'Z' ? matrix.m43 : Math.atan2(matrix.m31, matrix.m11) * 180 / Math.PI;
        });
        transition.finish();
        if (duration !== 800) throw new Error(`unexpected authored duration ${duration}`);
        return { browser, rootFontSize: parseFloat(getComputedStyle(document.documentElement).fontSize) };
    }, { tag: step.tag, times: poses.map(pose => pose.sample_ms) });
    const native = poses.map(pose => {
        const [transform, extra] = Object.values(pose.transforms);
        assert(!extra, 'the motion should have a single source transform');
        const match = step.tag === 'Z'
            ? /^translate3d\(0, 0, (-?[\d.]+)rem\)$/.exec(transform)
            : /^rotatey\((-?[\d.]+)deg\)$/.exec(transform);
        assert(match, `unexpected native local transform ${transform}`);
        return Number(match[1]) * (step.tag === 'Z' ? rootFontSize : 1);
    });
    const maximumError = Math.max(...native.map((value, i) => Math.abs(value - browser[i])));
    assert(maximumError < 0.005, `native camera diverged from Chromium by ${maximumError}`);
    if (step.tag === 'X') assert(Math.max(...native) > 92, 'the source right turn must overshoot 90°');
    return Number(maximumError.toFixed(5));
}

await withBackrooms(async ({ page, reset, snapshot, htmlPath, cssPath }) => {
    const out = await mkdtemp(join(tmpdir(), 'css-gpui-motion-'));
    try {
        const sample = join(out, 'motion-x11');
        run('cc', ['-Wall', '-Wextra', '-Werror', '-O2', source, '-o', sample,
            ...run('pkg-config', ['--cflags', '--libs', 'x11', 'xtst']).split(/\s+/)]);
        for (const [width, height] of viewports) {
            const states = steps.map(({ name }, index) => {
                const rgba = join(out, `${width}x${height}-${name}.rgba`);
                const { controls } = snapshot('--planar-preview', width, height,
                    ['play', ...steps.slice(0, index + 1).map(step => step.target)],
                    JSON.stringify({ root: 'content', label_scope: 'body', rgba }));
                return { ...controls, rgba };
            });
            const native = { width, height, html: htmlPath, css: cssPath, sample, out, states };
            const evidence = await testNative(native);
            await testNative({ ...native, interrupt: true });
            await reset(width, height);
            await page.locator('start-text label[for=play]').click();
            await page.evaluate(() => {
                // Enable authored transitions after settling Enter and keep
                // keyframes frozen to isolate the two movement transitions.
                const suppression = [...document.querySelectorAll('style')].find(style =>
                    style.textContent === '* { transition: none !important; animation: none !important; }');
                suppression.textContent = '*, *::before, *::after { animation: none !important; }';
            });
            for (const [index, step] of steps.entries()) {
                const { readback, poses } = evidence[index];
                const chromiumPoseError = await compareChromium(page, step, poses);
                console.log(JSON.stringify({
                    width, height, move: step.name, ...readback, comparedPoses: poses.length,
                    meanGeneratedMs: Number((poses.reduce((sum, pose) => sum + pose.generated_ms, 0) / poses.length).toFixed(2)),
                    firstSubmittedMs: Number(poses[0].submitted_ms.toFixed(2)),
                    chromiumPoseError, exactEndpoint: true, interruptedRouteSettledExactly: true,
                }));
            }
        }
    } finally {
        await rm(out, { recursive: true, force: true });
    }
});
