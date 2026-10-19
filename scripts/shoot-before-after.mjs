// Capture before/after shots of the four views.
//
// "Before" is `main`'s frontend, extracted to a temp dir so the current working tree is
// untouched. Both sides run through the same stub and viewport, so the pair is comparable.
import { chromium } from "@playwright/test";
import { spawn, execFileSync } from "node:child_process";
import { mkdir, mkdtemp, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, "..");
const out = join(root, "docs", "ui");
await mkdir(out, { recursive: true });

const before = await mkdtemp(join(tmpdir(), "vp-before-"));
for (const f of ["index.html", "styles.css", "main.js"]) {
  const body = execFileSync("git", ["show", `main:src/${f}`], { cwd: root, encoding: "utf8" });
  await writeFile(join(before, f), body);
}

async function serve(dir) {
  const p = spawn(process.execPath, [join(here, "serve.mjs"), dir, "0"], {
    stdio: ["ignore", "pipe", "inherit"],
  });
  const port = await new Promise((r) => p.stdout.once("data", (d) => r(Number(d.toString().trim()))));
  return { port, stop: () => p.kill() };
}

const VIEWPORT = { width: 920, height: 680 };
const views = [
  ["nav-download", "pluck"],
  ["nav-queue", "queue"],
  ["nav-search", "search"],
  ["nav-settings", "settings"],
];

for (const [label, dir] of [
  ["before", before],
  ["after", join(root, "src")],
]) {
  const { port, stop } = await serve(dir);
  const browser = await chromium.launch();
  const page = await browser.newPage({ viewport: VIEWPORT });
  await page.addInitScript({ path: join(root, "tests", "tauri-stub.js") });
  await page.goto(`http://127.0.0.1:${port}/`);
  // main's frontend has no #plucks-empty and a different boot shape, so wait for the
  // nav rather than a card.
  await page.waitForSelector("#nav-download");
  await page.waitForTimeout(400);

  for (const [navId, name] of views) {
    await page.locator(`#${navId}`).click();
    await page.waitForTimeout(200);
    // Move off the tab: a click leaves the pointer on it and the hover fill is
    // indistinguishable from a selected fill in a still.
    await page.mouse.move(2, 2);
    await page.waitForTimeout(100);
    await page.screenshot({ path: join(out, `${name}-${label}.png`) });
    console.log(`${name}-${label}.png`);
  }

  await browser.close();
  stop();
}
