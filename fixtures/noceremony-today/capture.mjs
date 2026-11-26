import assert from "node:assert/strict";
import { mkdir, writeFile } from "node:fs/promises";
import { createRequire } from "node:module";
import { extname, resolve } from "node:path";
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

try {
    const captures = [];
    const snapshots = [];
    const fonts = new Map();
    for (let attempt = 0; attempt < 2; attempt++) {
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
                snapshots.push(await page.evaluate(snapshotScene));
                await Promise.all(fontRequests);
            }
        } finally {
            await page.close();
        }
    }

    assert(captures[0].equals(captures[1]), "Two fresh browser sessions produced different PNGs");
    if (outputDirectory) {
        assert.deepEqual(snapshots[0], snapshots[1], "Two browser sessions produced different Scene IR");
        const { scene, losses, fonts: loadedFonts } = snapshots[0];
        await mkdir(output, { recursive: true });
        for (const [name, data] of [
            ["scene.json", scene],
            ["losses.json", { losses, loadedFonts, scale, chrome: browser.version() }],
        ]) {
            await writeFile(resolve(output, name), JSON.stringify(data, null, 2) + "\n");
        }
        const fontDirectory = resolve(output, "fonts");
        await mkdir(fontDirectory, { recursive: true });
        for (const [index, [url, data]] of [...fonts].sort(([left], [right]) => left.localeCompare(right)).entries()) {
            await writeFile(resolve(fontDirectory, "webfont-" + index + extname(new URL(url).pathname)), data);
        }
        console.log(scene.nodes.length + " native nodes and " + losses.length + " reported snapshot losses");
    }
    await writeFile(resolve(output, "browser-reference.png"), captures[0]);
    console.log("Captured two identical Today frames with Chrome " + browser.version());
} finally {
    await browser.close();
}
