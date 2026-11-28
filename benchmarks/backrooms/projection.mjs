import assert from 'node:assert/strict';
import { withBackrooms, viewports } from './probe.mjs';

const steps = [
    ['play'], ['play', 'z-8'], ['play', 'z-8', 'right1'], ['play', 'z-8', 'right1', 'level-2'],
];
const tiles = [6, 7, 8, 9].flatMap(column =>
    [6, 7, 8, 9, 10].map(row => `content floor > tile.col-${column}.row-${row}`));
const candidates = ['content floor', ...tiles, ...tiles.map(tile => `${tile} > span`)];
const tolerance = 0.035;

await withBackrooms(async ({ page, snapshot, reset }) => {
    const cdp = await page.context().newCDPSession(page);
    await cdp.send('DOM.enable');
    const total = { vertices: 0, wallVertices: 0, viewportPolygons: 0,
        wallViewportPolygons: 0, clippedAtEye: 0, crossingEyeInViewport: 0, maxError: 0 };
    for (const [width, height] of viewports) {
        await reset(width, height);
        const { root } = await cdp.send('DOM.getDocument');
        for (const selection of steps) {
            await page.evaluate(ids => {
                for (const id of ids) document.getElementById(id).checked = true;
            }, selection);

            const { boxes, chains, reference, walls } = await measure(cdp, root.nodeId);
            const untransformed = await page.addStyleTag({
                content: `content, content *, content *::before, content *::after {
                    transform: none !important;
                    perspective: none !important;
                }`,
            });
            const bounds = {};
            try {
                for (const [key, id] of boxes) {
                    const [[left, top], [right], [, bottom]] = await borderQuad(cdp, id);
                    bounds[key] = { origin: [left, top], size: [right - left, bottom - top] };
                }
            } finally {
                await untransformed.evaluate(node => node.remove());
            }

            const projected = snapshot('--projection-snapshot', width, height, selection,
                JSON.stringify({ bounds, chains }));
            assert.deepEqual(Object.keys(projected).sort(), [...reference.keys()].sort());
            const stats = { vertices: 0, wallVertices: 0, viewportPolygons: 0,
                wallViewportPolygons: 0, clippedAtEye: 0,
                crossingEyeInViewport: 0, outsideViewport: 0, wallsInViewport: 0, maxError: 0 };
            for (const [key, { corners, viewport_polygon: polygon }] of Object.entries(projected)) {
                const expected = reference.get(key);
                const info = `${width}x${height} ${selection.join(' ')} ${key}`;
                const compare = (actual, target) => {
                    const error = Math.hypot(actual[0] - target[0], actual[1] - target[1]);
                    assert(error <= tolerance, `${info}: ${actual} vs ${target} (error ${error}px)`);
                    stats.maxError = Math.max(stats.maxError, error);
                };
                let finite = 0;
                corners.forEach((corner, index) => {
                    if (corner) {
                        compare(corner, expected[index]);
                        finite++;
                    }
                });
                stats.vertices += finite;
                if (walls.has(key)) stats.wallVertices += finite;
                if (finite === 4) {
                    const expectedPolygon = clip(expected, width, height);
                    assert.equal(polygon.length, expectedPolygon.length, info);
                    polygon.forEach((point, index) => compare(point, expectedPolygon[index]));
                    stats.viewportPolygons++;
                    if (walls.has(key)) stats.wallViewportPolygons++;
                } else {
                    stats.clippedAtEye++;
                    if (polygon.length) stats.crossingEyeInViewport++;
                }
                if (!polygon.length) stats.outsideViewport++;
                else if (walls.has(key)) stats.wallsInViewport++;
                for (const [x, y] of polygon) {
                    assert(x >= -tolerance && x <= width + tolerance
                        && y >= -tolerance && y <= height + tolerance, info);
                }
            }
            const name = `${width}x${height} ${selection.join(' ')}`;
            assert(stats.vertices >= 40, `${name}: too few vertices checked`);
            assert(stats.viewportPolygons >= 6, `${name}: too few complete polygons checked`);
            assert(stats.wallVertices >= 8 && stats.wallsInViewport > 0, `${name}: too few corner walls checked`);
            assert(stats.crossingEyeInViewport > 0, `${name}: no mixed eye-plane sample intersects the viewport`);
            for (const key of Object.keys(total).filter(key => key !== 'maxError')) {
                total[key] += stats[key];
            }
            total.maxError = Math.max(total.maxError, stats.maxError);
            console.log(JSON.stringify({ width, height, selection, ...stats }));
        }
    }
    console.log(JSON.stringify({ total }));
});

async function measure(cdp, root) {
    const find = async selector => (await cdp.send('DOM.querySelector', { nodeId: root, selector })).nodeId;
    const paths = await cdp.send('Runtime.evaluate', {
        expression: `(() => {
            const scene = document.querySelector('content');
            const path = el => {
                let ordinal = 1;
                for (let sibling = el.previousElementSibling; sibling; sibling = sibling.previousElementSibling) {
                    if (sibling.localName === el.localName) ordinal++;
                }
                return [el.parentElement && path(el.parentElement), el.localName + ':' + ordinal].join('/');
            };
            return ${JSON.stringify(candidates)}.map(selector => {
                const element = document.querySelector(selector);
                if (!element?.getClientRects().length) return null;
                const chain = [];
                for (let el = element; el; el = el === scene ? null : el.parentElement) {
                    chain.unshift(path(el));
                }
                const wallFaces = ['before', 'after'].filter(pseudo => {
                    const style = getComputedStyle(element, '::' + pseudo);
                    if (style.display === 'none' || style.transform === 'none') return false;
                    const m = new DOMMatrixReadOnly(style.transform);
                    return Math.abs(m.m13) > 0.99 || Math.abs(m.m23) > 0.99;
                });
                return [selector, chain, wallFaces];
            }).filter(Boolean);
        })()`,
        returnByValue: true,
    });
    assert(!paths.exceptionDetails, JSON.stringify(paths.exceptionDetails));
    const boxes = new Map();
    const chains = [];
    const reference = new Map();
    const walls = new Set();
    for (const [selector, chain, wallFaces] of paths.result.value) {
        for (const key of chain) {
            if (boxes.has(key)) continue;
            const css = key.slice(1).split('/').map(part => part.replace(/:(\d+)$/, ':nth-of-type($1)')).join(' > ');
            const node = (await cdp.send('DOM.describeNode', { nodeId: await find(css) })).node;
            boxes.set(key, node.backendNodeId);
        }
        const key = chain.at(-1);
        chains.push(chain);
        reference.set(key, await borderQuad(cdp, boxes.get(key)));
        if (!selector.includes('tile.') || selector.endsWith(' > span')) continue;
        const { node } = await cdp.send('DOM.describeNode', { backendNodeId: boxes.get(key) });
        for (const pseudo of node.pseudoElements ?? []) {
            if (!['before', 'after'].includes(pseudo.pseudoType)) continue;
            const pseudoKey = `${key}::${pseudo.pseudoType}`;
            boxes.set(pseudoKey, pseudo.backendNodeId);
            chains.push([...chain, pseudoKey]);
            reference.set(pseudoKey, await borderQuad(cdp, pseudo.backendNodeId));
            if (wallFaces.includes(pseudo.pseudoType)) walls.add(pseudoKey);
        }
    }
    return { boxes, chains, reference, walls };
}

async function borderQuad(cdp, backendNodeId) {
    const { model } = await cdp.send('DOM.getBoxModel', { backendNodeId });
    return [0, 2, 4, 6].map(index => model.border.slice(index, index + 2));
}

// Chromium provides finite screen vertices. Clip those in two dimensions
// independently of the Rust homogeneous clipping path.
function clip(corners, width, height) {
    let points = corners;
    for (const [axis, edge, side] of [[0, 0, 1], [0, width, -1], [1, 0, 1], [1, height, -1]]) {
        const polygon = [];
        for (let i = 0; i < points.length; i++) {
            const current = points[i];
            const previous = points.at(i - 1);
            const currentInside = side * (current[axis] - edge) >= 0;
            const previousInside = side * (previous[axis] - edge) >= 0;
            if (currentInside !== previousInside) {
                const other = 1 - axis;
                const ratio = (edge - previous[axis]) / (current[axis] - previous[axis]);
                const intersection = [];
                intersection[axis] = edge;
                intersection[other] = previous[other] + ratio * (current[other] - previous[other]);
                polygon.push(intersection);
            }
            if (currentInside) polygon.push(current);
        }
        points = polygon;
    }
    return points;
}
