import assert from "node:assert/strict";
import { writeFile } from "node:fs/promises";
import { createRequire } from "node:module";
import { resolve } from "node:path";

const [noceremony, appUrl = "http://127.0.0.1:4176/"] = process.argv.slice(2);
if (!noceremony) {
    throw new Error("Usage: node capture.mjs <noceremony checkout> [app URL]");
}

const require = createRequire(resolve(noceremony, "package.json"));
const { chromium } = require("playwright");
const browser = await chromium.launch({ channel: "chrome" });

try {
    const captures = [];
    for (let attempt = 0; attempt < 2; attempt++) {
        const page = await browser.newPage({
            viewport: { width: 1280, height: 900 },
            deviceScaleFactor: 1,
            timezoneId: "America/Los_Angeles",
            locale: "en-US",
            colorScheme: "light",
            reducedMotion: "reduce",
        });
        try {
            await page.clock.setFixedTime(new Date("2026-11-25T18:30:00Z"));
            await page.goto(appUrl);
            await page.getByRole("button", { name: "Load a fake day" }).click();
            await page.locator("#commandCard .task-row").waitFor();
            await page.evaluate(() => document.fonts.ready);
            await page.mouse.move(0, 0);
            captures.push(await page.screenshot({ animations: "disabled" }));
        } finally {
            await page.close();
        }
    }

    assert(captures[0].equals(captures[1]), "Two fresh browser sessions produced different PNGs");
    await writeFile(new URL("./browser-reference.png", import.meta.url), captures[0]);
    console.log("Captured two identical Today frames with Chrome " + browser.version());
} finally {
    await browser.close();
}
