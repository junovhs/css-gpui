import assert from "node:assert/strict";
import { writeFile } from "node:fs/promises";
import { resolve } from "node:path";
import { createRequire } from "node:module";
import { fileURLToPath } from "node:url";

const [playwrightProject, scaleArg = "1", outputPath] = process.argv.slice(2);
assert(playwrightProject, "Usage: node capture.mjs <project with Playwright installed>");
const require = createRequire(resolve(playwrightProject, "package.json"));
const { chromium } = require("playwright");
const source = new URL("./source.html", import.meta.url);
const output = name => outputPath ? resolve(outputPath, name) : fileURLToPath(new URL(name, import.meta.url));
const scale = Number(scaleArg);
assert(Number.isFinite(scale) && scale > 0, "Expected a positive device scale");
const browser = await chromium.launch({
    channel: "chrome",
    args: ["--allow-file-access-from-files"],
});
const widths = [360, 720];
const titles = ["Files", "Recently opened files"];
const toolbarHeight = 56;
const caseHeight = 64;

try {
    const page = await browser.newPage({
        viewport: { width: 720, height: caseHeight * widths.length * titles.length },
        deviceScaleFactor: scale,
    });
    await page.goto(source.href);
    const frames = await page.evaluate(async ({ widths, titles, toolbarHeight, caseHeight }) => {
        document.body.replaceChildren();
        document.body.style.backgroundColor = "#e7eaf0";
        const cases = [];
        for (const title of titles) {
            for (const width of widths) {
                const frame = document.createElement("iframe");
                frame.style.cssText = `display:block;border:0;margin-bottom:${caseHeight - toolbarHeight}px`;
                frame.width = width;
                frame.height = toolbarHeight;
                const ready = new Promise(resolve => frame.addEventListener("load", resolve, { once: true }));
                frame.src = location.href;
                document.body.append(frame);
                await ready;

                const doc = frame.contentDocument;
                doc.querySelector(".title").textContent = title;
                await doc.fonts.ready;
                const measure = selector => {
                    const { x, y, width, height } = doc.querySelector(selector).getBoundingClientRect();
                    return { x, y, width, height };
                };
                cases.push({
                    title,
                    width,
                    atlas: { x: 0, y: frame.getBoundingClientRect().y, width, height: toolbarHeight },
                    toolbar: measure(".toolbar"),
                    titleBounds: measure(".title"),
                    button: measure(".action"),
                });
            }
        }
        return cases;
    }, { widths, titles, toolbarHeight, caseHeight });

    for (const { width, toolbar, titleBounds, button } of frames) {
        assert.equal(toolbar.width, width);
        assert.equal(titleBounds.x, 16);
        assert.equal(width - (button.x + button.width), 16);
        assert.equal(titleBounds.y + titleBounds.height / 2, toolbarHeight / 2);
        assert.equal(button.y + button.height / 2, toolbarHeight / 2);
        assert(titleBounds.x + titleBounds.width < button.x);
    }
    await page.screenshot({ path: output("browser-reference.png") });
    await writeFile(output("browser-bounds.json"), JSON.stringify({
        chrome: browser.version(),
        font: "Arial",
        scale,
        cases: frames,
    }, null, 2) + "\n");
} finally {
    await browser.close();
}
