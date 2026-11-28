import { comparePositionedBoxes } from './positioned-box-probe.mjs';

await comparePositionedBoxes('aspect-ratio', [
    { selector: '#phone', color: '#e02040' },
    { selector: '#control', color: '#20a060' },
    { selector: '#border-floor', color: '#4050e0' },
]);
