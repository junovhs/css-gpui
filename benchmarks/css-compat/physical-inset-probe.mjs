import { comparePositionedBoxes } from './positioned-box-probe.mjs';

await comparePositionedBoxes('physical-inset', [
    { selector: '#overlay', color: '#102030', background: true },
    { selector: '#stretched', color: '#e02040' },
    { selector: '#trailing', color: '#20a060' },
    { selector: '#overconstrained', color: '#4050e0' },
    { selector: '#flex-stretch', color: '#8030a0' },
    { selector: '#fixed', color: '#b06810' },
    { selector: '#absolute-root', color: '#106080' },
]);
