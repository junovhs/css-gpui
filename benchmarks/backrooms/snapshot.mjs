import assert from 'node:assert/strict';
import { withBackrooms, viewports } from './probe.mjs';

const steps = [[], ['play'], ['play', 'z-8'], ['play', 'z-8', 'right1'],
    ['play', 'z-8', 'right1', 'level-2']];
await withBackrooms(async ({ page, snapshot, reset }) => {
    for (const [width, height] of viewports) {
        await reset(width, height);
        for (const selection of steps) {
            const actual = snapshot('--radio-snapshot', width, height, selection);
            const browserSnapshot = await page.evaluate(ids => {
                for (const id of ids) document.getElementById(id).checked = true;
                const style = (selector, pseudo) => getComputedStyle(document.querySelector(selector), pseudo);
                const transform = selector => {
                    const value = style(selector).transform;
                    return new DOMMatrixReadOnly(value === 'none' ? undefined : value);
                };
                const turn = transform('content > w > x');
                const background = style('html').backgroundColor.match(/\d+/g);
                return {
                    menuOffset: transform('buttons').m41,
                    overlayPosition: style('bh').position,
                    walk: transform('content > w > x > y > z').m43,
                    turn: Math.atan2(turn.m31, turn.m11) * 180 / Math.PI,
                    background: '#' + background.map(channel => Number(channel).toString(16).padStart(2, '0')).join(''),
                    buttons: [...document.querySelectorAll('buttons > label')]
                        .filter(node => getComputedStyle(node).display !== 'none')
                        .map(node => [node.className, node.htmlFor]),
                    levelLabel: style('label[for="level-2"]').display,
                    levelLabelContent: style('label[for="level-2"]', '::before').content,
                };
            }, selection);
            const resolvedSnapshot = readResolvedSnapshot(actual);
            const name = `${width}x${height}, ${selection.join(' ') || 'authored defaults'}`;
            assert.deepEqual(resolvedSnapshot, browserSnapshot, name);
            console.log(JSON.stringify({ width, height, selection, ...resolvedSnapshot }));
        }
    }
});

function readResolvedSnapshot(nodes) {
    const first = tag => nodes.find(node => node.tag === tag);
    const transform = node => node.projection.transform;
    const translation = (node, axis) => {
        const value = transform(node);
        if (value === undefined || value === 'none') return 0;
        const args = /^translate3d\(([^)]+)\)$/.exec(value)?.[1];
        assert(args, `Expected a single translate3d for <${node.tag}>, received ${value}`);
        const coordinate = args.split(',')[axis].trim();
        if (coordinate === '0') return 0;
        assert(coordinate.endsWith('rem'), `Expected a rem translation, received ${coordinate}`);
        return Number(coordinate.slice(0, -3)) * (first('html').font_size ?? 16);
    };
    const turn = transform(first('x'));
    const degrees = turn === undefined || turn === 'none' ? 0 : /^rotateY\((-?\d+)deg\)$/.exec(turn)?.[1];
    assert(degrees !== undefined, `Expected a single rotateY, received ${turn}`);
    const buttons = first('buttons');
    const label = nodes.find(node => node.tag === 'label' && node.attributes.for === 'level-2');
    return {
        menuOffset: translation(buttons, 0),
        overlayPosition: first('bh').position,
        walk: translation(first('z'), 2),
        turn: Number(degrees),
        background: first('html').background_color,
        buttons: nodes.filter(node => node.parent_id === buttons.id && node.tag === 'label' && node.display !== 'none')
            .map(node => [node.attributes.class, node.attributes.for]),
        levelLabel: label.display,
        levelLabelContent: label.decorations.find(decoration => decoration.pseudo === 'before').effects.content,
    };
}
