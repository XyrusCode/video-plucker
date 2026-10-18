import { test, expect } from "@playwright/test";
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const here = dirname(fileURLToPath(import.meta.url));

/**
 * The contract checks. These fail on a design change that the DESIGN.md review
 * missed, which is the whole point: a screenshot diff catches what looks wrong,
 * a token assertion catches what drifted.
 */

// From DESIGN.md section 2 (Theme.kt). Hex, because an unregistered custom
// property resolves to its declared token rather than to rgb().
const CONTRACT = {
  "--accent": "#ff2d55", // #FF2D55
  "--bg": "#0d0d0f", // #0D0D0F
  "--panel": "#1c1c1e", // #1C1C1E
  "--panel-2": "#2c2c2e", // #2C2C2E
  "--text": "#f5f5f7", // #F5F5F7
  "--ok": "#30d158", // #30D158
  "--warn": "#ff9f0a", // #FF9F0A
};

test.beforeEach(async ({ page }) => {
  const errors = [];
  page.on("pageerror", (e) => errors.push(String(e)));
  page.on("console", (m) => {
    if (m.type() === "error") errors.push(m.text());
  });
  page.__errors = errors;

  await page.addInitScript({ path: join(here, "tauri-stub.js") });
  await page.goto("/");
  // The app boots asynchronously (three stores, then renders).
  await expect(page.locator("#plucks .job-card")).toHaveCount(1);
});

test.afterEach(async ({ page }) => {
  expect(page.__errors, "page logged errors during the test").toEqual([]);
});

test("design tokens match DESIGN.md", async ({ page }) => {
  const actual = await page.evaluate(() => {
    const styles = getComputedStyle(document.documentElement);
    const out = {};
    for (const name of Object.keys({
      "--accent": 1,
      "--bg": 1,
      "--panel": 1,
      "--panel-2": 1,
      "--text": 1,
      "--ok": 1,
      "--warn": 1,
    })) {
      out[name] = styles.getPropertyValue(name).trim().toLowerCase();
    }
    return out;
  });

  expect(actual).toEqual(CONTRACT);
});

test("the accent is the phone's red, not the old blue", async ({ page }) => {
  // The desktop app was #0a84ff throughout. One assertion names that directly so a
  // regression says what regressed.
  const accent = await page.evaluate(() =>
    getComputedStyle(document.documentElement)
      .getPropertyValue("--accent")
      .trim()
      .toLowerCase(),
  );
  expect(accent).not.toContain("0a84ff");
});

test("the accent token actually reaches a painted element", async ({ page }) => {
  // A token can be correct in :root and still be unused, which is how the pink logo
  // gradient survived as the app's only brand colour for a whole release.
  const painted = await page.evaluate(() => {
    const probe = document.createElement("span");
    probe.style.color = "var(--accent)";
    document.body.appendChild(probe);
    const value = getComputedStyle(probe).color;
    probe.remove();
    return value;
  });
  expect(painted).toBe("rgb(255, 45, 85)");
});

test("every cached getElementById in main.js resolves", async ({ page }) => {
  // main.js caches ~40 elements at module scope and dereferences them immediately.
  // A renamed id is a null dereference on boot, which this catches before a user does.
  const source = await readFile(join(here, "..", "src", "main.js"), "utf8");
  const ids = [...source.matchAll(/getElementById\("([^"]+)"\)/g)].map((m) => m[1]);

  expect(ids.length).toBeGreaterThan(30);
  const missing = await page.evaluate(
    (wanted) => wanted.filter((id) => document.getElementById(id) === null),
    ids,
  );
  expect(missing, "ids referenced by main.js but absent from index.html").toEqual([]);
});

test("dead classes are gone", async ({ page }) => {
  // .glossy-btn shipped on 20 elements with no CSS rule behind it after the 4.7.0
  // rewrite. Harmless today, a trap the next person copies.
  const glossy = await page.evaluate(
    () => document.querySelectorAll(".glossy-btn").length,
  );
  expect(glossy).toBe(0);
});

test("each tab shows its view and moves the active pill", async ({ page }) => {
  const tabs = [
    ["nav-download", "view-download"],
    ["nav-queue", "view-queue"],
    ["nav-search", "view-search"],
    ["nav-settings", "view-settings"],
  ];

  for (const [navId, viewId] of tabs) {
    await page.locator(`#${navId}`).click();
    await expect(page.locator(`#${viewId}`)).toBeVisible();
    await expect(page.locator(`#${navId}`)).toHaveClass(/active/);
    // Exactly one pill is active at a time.
    await expect(page.locator(".pill-chip.active")).toHaveCount(1);
  }
});

test("downloads live under Pluck and nowhere else", async ({ page }) => {
  // Mobile shows downloads inside the Pluck tab. They used to sit outside the view
  // system, so they rendered on the Search and Settings tabs too.
  const insidePluck = await page.evaluate(
    () => !!document.querySelector("#view-download #plucks"),
  );
  expect(insidePluck).toBe(true);

  for (const viewId of ["view-queue", "view-search", "view-settings"]) {
    const leaked = await page.evaluate(
      (id) => !!document.querySelector(`#${id} #plucks`),
      viewId,
    );
    expect(leaked, `#plucks leaked into #${viewId}`).toBe(false);
  }
});

test("every screen has a title and a subtitle", async ({ page }) => {
  const views = [
    "view-download",
    "view-queue",
    "view-search",
    "view-settings",
  ];
  for (const id of views) {
    const texts = await page.evaluate((viewId) => {
      const view = document.getElementById(viewId);
      return {
        title: view.querySelector(".screen-title")?.textContent?.trim() ?? "",
        sub: view.querySelector(".screen-sub")?.textContent?.trim() ?? "",
      };
    }, id);
    expect(texts.title, `#${id} has no screen title`).not.toBe("");
    expect(texts.sub, `#${id} has no screen subtitle`).not.toBe("");
  }
});

test("a job row does not overflow its card", async ({ page }) => {
  // The controls used to be bare siblings in a space-between row with no wrap, which
  // overflowed once more than three were showing at once.
  const overflow = await page.evaluate(() => {
    const head = document.querySelector("#plucks .job-head");
    return head.scrollWidth - head.clientWidth;
  });
  expect(overflow).toBeLessThanOrEqual(0);
});

test("export sends the arguments the Rust command declares", async ({ page }) => {
  // Tauri rejects a call missing one of the command's parameters at the boundary, with
  // an error that reads like a filesystem failure. export_dump takes settings and
  // parked as well as a path; sending only the path fails after the file dialog closes,
  // which is the worst possible moment to find out.
  await page.locator("#nav-settings").click();
  await page.locator("#export-dump-btn").click();
  await expect(page.locator("#dump-status")).toBeVisible();

  const call = await page.evaluate(
    () => window.__invokeLog.find((c) => c.cmd === "export_dump") ?? null,
  );
  expect(call, "export_dump was never invoked").not.toBeNull();
  expect(Object.keys(call.args).sort()).toEqual(["parked", "path", "settings"]);
  expect(call.args.parked).toHaveLength(1);
  expect(call.args.settings).toHaveProperty("autoUpdate");
});

test("import persists what the command returned", async ({ page }) => {
  // The settings store is a frontend store, so the command cannot write it. If the
  // returned prefs are dropped on the floor the import reports success and restores
  // nothing, which is the worst failure mode this feature has.
  await page.locator("#nav-settings").click();
  await page.locator("#import-dump-btn").click();
  await expect(page.locator("#dump-status")).toContainText("1 setting");

  const saved = await page.evaluate(async () => {
    const store = await window.__TAURI__.store.load("settings.json");
    return store.get("autoUpdate");
  });
  expect(saved).toBe(false);

  const queued = await page.evaluate(async () => {
    const store = await window.__TAURI__.store.load("queue.json");
    const entries = await store.entries();
    return entries.map(([k]) => k);
  });
  expect(queued).toContain("restored-1");
});

test("a restored queue item downloads into the current folder, not an empty one", async ({
  page,
}) => {
  // restored_queue_items emits destDir: "" because the dump format has nowhere to put one.
  // Passed through untouched it becomes `yt-dlp -P ""`, which resolves to whatever
  // directory the app happened to launch in.
  await page.locator("#nav-settings").click();
  await page.locator("#import-dump-btn").click();
  await expect(page.locator("#dump-status")).toContainText("1 setting");

  await page.locator("#nav-queue").click();
  await page.locator(".queue-start").first().click();

  const started = await page.evaluate(() => {
    const call = window.__invokeLog.find((c) => c.cmd === "start_pluck");
    return call?.args ?? null;
  });
  expect(started, "start_pluck was never invoked").not.toBeNull();
  expect(started.destDir, "restored item kept an empty destDir").toBe(
    "C:\\Users\\test\\Videos",
  );
});

test("screenshots", async ({ page }, testInfo) => {
  // Doubles as the after-shot for the UI change, so the shot harness and the
  // regression harness stay one thing.
  for (const [navId, name] of [
    ["nav-download", "pluck"],
    ["nav-queue", "queue"],
    ["nav-search", "search"],
    ["nav-settings", "settings"],
  ]) {
    await page.locator(`#${navId}`).click();
    await page.waitForTimeout(120);
    await testInfo.attach(`${name}.png`, {
      body: await page.screenshot({ fullPage: true }),
      contentType: "image/png",
    });
  }
});
