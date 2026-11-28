import assert from 'node:assert/strict';
import { withBackrooms, viewports } from './probe.mjs';
import { measure, measureUsedBoxes } from './geometry.mjs';

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
    const total = { boxes: 0, maxBoxError: 0, vertices: 0, wallVertices: 0, viewportPolygons: 0,
        wallViewportPolygons: 0, clippedAtEye: 0, crossingEyeInViewport: 0, maxError: 0 };
    for (const [width, height] of viewports) {
        await reset(width, height);
        const { root } = await cdp.send('DOM.getDocument');
        for (const selection of steps) {
            await page.evaluate(ids => {
                for (const id of ids) document.getElementById(id).checked = true;
            }, selection);

            const { boxes, chains, reference, walls } = await measure(cdp, root.nodeId, candidates);
            const referenceBounds = await measureUsedBoxes(page, cdp, boxes);

            const { bounds, projected } = snapshot('--projection-snapshot', width, height, selection,
                JSON.stringify({ chains }));
            const name = `${width}x${height} ${selection.join(' ')}`;
            assert.deepEqual(Object.keys(bounds).sort(), Object.keys(referenceBounds).sort(), name);
            let maxBoxError = 0;
            for (const [key, box] of Object.entries(bounds)) {
                const expected = referenceBounds[key];
                for (const property of ['origin', 'size']) {
                    const error = Math.hypot(...box[property].map((value, axis) => value - expected[property][axis]));
                    assert(error <= 1 / 64, `${name} ${key} ${property}: ${box[property]} vs ${expected[property]}`);
                    maxBoxError = Math.max(maxBoxError, error);
                }
            }
            assert.deepEqual(Object.keys(projected).sort(), [...reference.keys()].sort());
            const stats = { boxes: Object.keys(bounds).length, maxBoxError,
                vertices: 0, wallVertices: 0, viewportPolygons: 0,
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
            assert(stats.vertices >= 40, `${name}: too few vertices checked`);
            assert(stats.viewportPolygons >= 6, `${name}: too few complete polygons checked`);
            assert(stats.wallVertices >= 8 && stats.wallsInViewport > 0, `${name}: too few corner walls checked`);
            assert(stats.crossingEyeInViewport > 0, `${name}: no mixed eye-plane sample intersects the viewport`);
            for (const key of Object.keys(total).filter(key => !key.startsWith('max'))) {
                total[key] += stats[key];
            }
            total.maxError = Math.max(total.maxError, stats.maxError);
            total.maxBoxError = Math.max(total.maxBoxError, stats.maxBoxError);
            console.log(JSON.stringify({ width, height, selection, ...stats }));
        }
    }
    console.log(JSON.stringify({ total }));
});

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
