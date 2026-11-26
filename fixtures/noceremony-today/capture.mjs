import assert from 'node:assert/strict';
import { isDeepStrictEqual } from 'node:util';
import { mkdir, writeFile } from "node:fs/promises";
import { createRequire } from "node:module";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { snapshotScene } from "./snapshot.mjs";

const [noceremony, appUrl = "http://127.0.0.1:4176/", outputDirectory, scaleArg = "1"] = process.argv.slice(2);
if (!noceremony) {
    throw new Error("Usage: node capture.mjs <noceremony checkout> [app URL] [snapshot directory] [scale]");
}

const output = outputDirectory ?? fileURLToPath(new URL(".", import.meta.url));
const scale = Number(scaleArg);
assert(Number.isFinite(scale) && scale > 0, "Expected a positive device scale");
const require = createRequire(resolve(noceremony, "package.json"));
const { chromium } = require("playwright");
const browser = await chromium.launch({ channel: "chrome" });

async function generatedText(page) {
    const session = await page.context().newCDPSession(page);
    try {
        const { documents: [document], strings } = await session.send("DOMSnapshot.captureSnapshot", {
            computedStyles: [],
        });
        const { nodes, layout } = document;
        const pseudos = nodes.pseudoType ?? { index: [], value: [] };
        const pseudoTypes = new Map(pseudos.index.map((index, position) => [
            index, strings[pseudos.value[position]],
        ]));
        const counts = new Map();
        const paths = [];
        for (const [index, nameIndex] of nodes.nodeName.entries()) {
            const name = strings[nameIndex].toLowerCase();
            const parent = nodes.parentIndex[index];
            const parentPath = paths[parent];
            const type = pseudoTypes.get(index);
            let path;
            if (name === "html") path = "/html";
            else if (name === "body" && parentPath === "/html") path = "/html/body";
            else if (parentPath && type) path = parentPath + "::" + type;
            else if (parentPath && nodes.nodeType[index] === 1) {
                const siblingKey = parent + "\0" + name;
                const ordinal = (counts.get(siblingKey) || 0) + 1;
                counts.set(siblingKey, ordinal);
                path = parentPath + "/" + name + "[" + ordinal + "]";
            }
            paths.push(path);
        }
        const fragments = {};
        for (const [index, node] of layout.nodeIndex.entries()) {
            if (!pseudoTypes.has(node) || !paths[node]) continue;
            const text = strings[layout.text[index]];
            if (!text?.trim()) continue;
            const [x, y, width, height] = layout.bounds[index];
            (fragments[paths[node]] ??= []).push({
                text, x: x - (document.scrollOffsetX ?? 0), y: y - (document.scrollOffsetY ?? 0), width, height,
            });
        }
        return fragments;
    } finally {
        await session.detach();
    }
}

try {
    const captures = [];
    let matched;
    const snapshots = [];
    const fonts = new Map();
    for (let attempt = 0; attempt < 4 && matched === undefined; attempt++) {
        const page = await browser.newPage({
            viewport: { width: 1280, height: 900 },
            deviceScaleFactor: scale,
            timezoneId: "America/Los_Angeles",
            locale: "en-US",
            colorScheme: "light",
            reducedMotion: "reduce",
        });
        try {
            const fontRequests = [];
            if (outputDirectory) {
                page.on("response", response => {
                    if (response.ok() && response.request().resourceType() === "font" && !fonts.has(response.url())) {
                        fontRequests.push(response.body().then(bytes => fonts.set(response.url(), bytes)));
                    }
                });
            }
            await page.clock.setFixedTime(new Date("2026-11-25T18:30:00Z"));
            await page.goto(appUrl);
            await page.getByRole("button", { name: "Load a fake day" }).click();
            await page.locator("#commandCard .task-row").waitFor();
            await page.evaluate(() => document.fonts.ready);
            await page.mouse.move(0, 0);
            captures.push(await page.screenshot({ animations: "disabled" }));
            if (outputDirectory) {
                snapshots.push(await page.evaluate(snapshotScene, await generatedText(page)));
                await Promise.all(fontRequests);
            }
        } finally {
            await page.close();
        }
        const last = captures.length - 1;
        const earlier = captures.findIndex((png, index) => index < last
            && png.equals(captures[last])
            && (!outputDirectory || isDeepStrictEqual(snapshots[index], snapshots[last])));
        if (earlier !== -1) matched = earlier;
    }

    assert(matched !== undefined, "No two browser sessions produced identical PNGs and Scene IR");
    if (outputDirectory) {
        const { scene, losses, fonts: loadedFonts } = snapshots[matched];
        await mkdir(output, { recursive: true });
        for (const [name, data] of [
            ["scene.json", scene],
            ["losses.json", { losses, loadedFonts, scale, chrome: browser.version() }],
        ]) {
            await writeFile(resolve(output, name), JSON.stringify(data, null, 2) + "\n");
        }
        const fontDirectory = resolve(output, "fonts");
        await mkdir(fontDirectory, { recursive: true });
        const formats = new Map([["wOF2", ".woff2"], ["wOFF", ".woff"], ["OTTO", ".otf"], ["\0\x01\0\0", ".ttf"]]);
        for (const [index, [url, data]] of [...fonts].sort(([left], [right]) => left.localeCompare(right)).entries()) {
            const extension = formats.get(data.toString("latin1", 0, 4));
            assert(extension, "Unknown browser font signature from " + url);
            await writeFile(resolve(fontDirectory, "webfont-" + index + extension), data);
        }
        console.log(scene.nodes.length + " native nodes and " + losses.length + " reported snapshot losses");
    }
    await writeFile(resolve(output, "browser-reference.png"), captures[matched]);
    console.log("Captured two identical Today frames with Chrome " + browser.version());
} finally {
    await browser.close();
}
