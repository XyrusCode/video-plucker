// One-off: write the four view screenshots to ./screenshots for review.
// Usage: node scripts/shoot.mjs
import { chromium } from "@playwright/test";
import { spawn } from "node:child_process";
import { mkdir } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, "..");
const out = join(root, "screenshots");
await mkdir(out, { recursive: true });

const server = spawn(process.execPath, [join(here, "serve.mjs"), join(root, "src"), "0"], {
  stdio: ["ignore", "pipe", "inherit"],
});
const port = await new Promise((resolve) => {
  server.stdout.once("data", (d) => resolve(Number(d.toString().trim())));
});

const browser = await chromium.launch();
const page = await browser.newPage({ viewport: { width: 1100, height: 1000 } });
await page.addInitScript({ path: join(root, "tests", "tauri-stub.js") });
await page.goto(`http://127.0.0.1:${port}/`);
await page.waitForSelector("#plucks .job-card");

for (const [navId, name] of [
  ["nav-download", "pluck"],
  ["nav-queue", "queue"],
  ["nav-search", "search"],
  ["nav-settings", "settings"],
]) {
  await page.click(`#${navId}`);
  await page.waitForTimeout(150);
  // Move the pointer off the tab. A click leaves the cursor on it, and the resting-state
  // hover fill is the thing you cannot tell apart from a selected fill in a still.
  await page.mouse.move(4, 4);
  await page.waitForTimeout(80);
  // Viewport, not fullPage: a full-page shot of a tall view gets downscaled hard enough
  // that the accent-on-near-black contrast is no longer readable.
  await page.screenshot({ path: join(out, `${name}.png`) });
  console.log(`wrote screenshots/${name}.png`);
}

await browser.close();
server.kill();
