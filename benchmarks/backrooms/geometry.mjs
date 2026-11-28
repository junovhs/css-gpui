import assert from 'node:assert/strict';

export async function measure(cdp, root, candidates) {
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

export async function measureUsedBoxes(page, cdp, boxes) {
    // The extra specificity outranks authored !important transforms.
    const measured = ':is(#__measure_a, *):is(#__measure_b, *):is(#__measure_c, *)';
    const untransformed = await page.addStyleTag({
        content: `content${measured}, content ${measured},
            content ${measured}::before, content ${measured}::after {
                transform: none !important;
                perspective: none !important;
            }`,
    });
    try {
        const bounds = {};
        for (const [key, id] of boxes) {
            const [[left, top], [right], [, bottom]] = await borderQuad(cdp, id);
            bounds[key] = { origin: [left, top], size: [right - left, bottom - top] };
        }
        return bounds;
    } finally {
        await untransformed.evaluate(node => node.remove());
    }
}
