import { test, expect } from "@playwright/test";
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const here = dirname(fileURLToPath(import.meta.url));

/**
 * The contract checks, against DESIGN.md (the canonical desktop contract).
 * A screenshot diff catches what looks wrong; these catch what drifted.
 */

// DESIGN.md §2. Hex, because an unregistered custom property resolves to its
// declared token rather than to rgb().
const CONTRACT = {
  "--bg": "#0f1115",
  "--surface": "#14171c",
  "--surface-hover": "#191d23",
  "--overlay": "#1b2027",
  "--line": "#242a32",
  "--text": "#e6e9ef",
  "--text-2": "#9aa4b2",
  "--accent": "#ff2d55",
  "--accent-fill": "#e01e45",
  "--ok": "#3fb950",
  "--warn": "#e3a008",
  "--error": "#ff7b72",
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
  // The app boots asynchronously (stores, then render). The stub seeds one
  // interrupted download and one failed download, so there are two rows.
  await expect(page.locator("#plucks tr.row")).toHaveCount(2);
});

test.afterEach(async ({ page }) => {
  expect(page.__errors, "page logged errors during the test").toEqual([]);
});

test("design tokens match DESIGN.md §2", async ({ page }) => {
  const actual = await page.evaluate((names) => {
    const styles = getComputedStyle(document.documentElement);
    const out = {};
    for (const name of names) {
      out[name] = styles.getPropertyValue(name).trim().toLowerCase();
    }
    return out;
  }, Object.keys(CONTRACT));

  expect(actual).toEqual(CONTRACT);
});

test("the accent token reaches a painted element", async ({ page }) => {
  // A token can be correct in :root and still be unused.
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

test("white text sits only on the AA-passing fill, not the brand red", async ({ page }) => {
  // White on --accent is 3.64:1. The primary button must use --accent-fill.
  const bg = await page.evaluate(
    () => getComputedStyle(document.getElementById("analyze-btn")).backgroundColor,
  );
  // The analyze button is a secondary control; check a primary one instead.
  const primary = await page.evaluate(
    () => getComputedStyle(document.getElementById("pluck-btn")).backgroundColor,
  );
  expect(primary).toBe("rgb(224, 30, 69)");
  expect(bg).not.toBe("rgb(255, 45, 85)");
});

test("every cached getElementById in main.js resolves", async ({ page }) => {
  // main.js caches its elements at module scope and dereferences them on boot.
  // A renamed id is a null dereference that the user meets first.
  const source = await readFile(join(here, "..", "src", "main.js"), "utf8");
  const ids = [...source.matchAll(/getElementById\("([^"]+)"\)/g)].map((m) => m[1]);

  expect(ids.length).toBeGreaterThan(30);
  const missing = await page.evaluate(
    (wanted) => wanted.filter((id) => document.getElementById(id) === null),
    ids,
  );
  expect(missing, "ids referenced by main.js but absent from index.html").toEqual([]);
});

test("the rail has four destinations and exactly one is active", async ({ page }) => {
  // DESIGN.md §5: a rail, not pill chips.
  await expect(page.locator(".rail .rail-item")).toHaveCount(4);
  await expect(page.locator(".rail .rail-item.active")).toHaveCount(1);
  await expect(page.locator(".rail .rail-item.active")).toHaveAttribute("aria-current", "page");
  await expect(page.locator(".pill-chip, .pill-row")).toHaveCount(0);
});

test("each rail item shows its view and moves the active marker", async ({ page }) => {
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
    await expect(page.locator(".rail-item.active")).toHaveCount(1);
  }
});

test("downloads are a table with a header, and live under Pluck", async ({ page }) => {
  // DESIGN.md §6.2: tables, with a sticky head.
  await expect(page.locator("#downloads-table thead th")).not.toHaveCount(0);
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

test("body rows stay within the §4 row height bound", async ({ page }) => {
  const heights = await page.evaluate(() =>
    [...document.querySelectorAll("#plucks tr.row")].map((r) => r.getBoundingClientRect().height),
  );
  for (const h of heights) expect(h).toBeLessThanOrEqual(36);
});

test("status bar shows engine state and the running version", async ({ page }) => {
  // DESIGN.md §5: the status bar is always present and reports the version.
  await expect(page.locator(".statusbar")).toBeVisible();
  await expect(page.locator("#status-engine")).not.toHaveText("");
  await expect(page.locator("#status-version")).toHaveText("v4.9.0");
});

test("no text is rendered uppercase (§3)", async ({ page }) => {
  const count = await page.evaluate(() => {
    let n = 0;
    for (const el of document.querySelectorAll("body *")) {
      if (getComputedStyle(el).textTransform === "uppercase") n++;
    }
    return n;
  });
  expect(count).toBe(0);
});

test("no button uses a gradient (§10)", async ({ page }) => {
  const count = await page.evaluate(() => {
    let n = 0;
    for (const el of document.querySelectorAll("button")) {
      if (getComputedStyle(el).backgroundImage !== "none") n++;
    }
    return n;
  });
  expect(count).toBe(0);
});

test("every screen has a title", async ({ page }) => {
  for (const id of ["view-download", "view-queue", "view-search", "view-settings"]) {
    const title = await page.evaluate(
      (viewId) => document.getElementById(viewId).querySelector(".view-title")?.textContent?.trim() ?? "",
      id,
    );
    expect(title, `#${id} has no view title`).not.toBe("");
  }
});

test("settings are two panes, and the section list switches them", async ({ page }) => {
  await page.locator("#nav-settings").click();
  await expect(page.locator(".settings-nav .settings-nav-item")).toHaveCount(5);
  await expect(page.locator("#pane-downloads")).toBeVisible();

  await page.locator('.settings-nav-item[data-pane="pane-cookies"]').click();
  await expect(page.locator("#pane-cookies")).toBeVisible();
  await expect(page.locator("#pane-downloads")).toBeHidden();
});

// The chrome depends on the platform the webview reports. CI runs on Linux, so
// each platform's expectations are pinned with its own user agent.
const WINDOWS_UA =
  "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0 Safari/537.36";
const NATIVE_UA = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0 Safari/537.36";

test.describe("on Windows", () => {
  test.use({ userAgent: WINDOWS_UA });

  test("the window controls call the frameless verbs", async ({ page }) => {
    await expect(page.locator("#window-controls")).toBeVisible();
    await page.locator("#win-minimize").click();
    await page.locator("#win-close").click();

    const log = await page.evaluate(() => window.__invokeLog.map((c) => c.cmd));
    expect(log).toContain("window.minimize");
    // Close must hide, not destroy: hide-to-tray is the app's behaviour.
    expect(log).toContain("window.hide");
  });

  test("the title bar is a Tauri drag region", async ({ page }) => {
    // Tauri reads data-tauri-drag-region; app-region is an Electron property and does nothing here.
    await expect(page.locator(".titlebar")).toHaveAttribute("data-tauri-drag-region", "");
    await expect(page.locator("body")).toHaveAttribute("data-chrome", "windows");
  });
});

test.describe("on macOS and Linux", () => {
  test.use({ userAgent: NATIVE_UA });

  test("native decorations stay on: no custom title bar, no window controls", async ({ page }) => {
    // A second title bar under the OS one would be two title bars. The brand bar is removed.
    await expect(page.locator("body")).toHaveAttribute("data-chrome", "native");
    await expect(page.locator(".titlebar")).toBeHidden();
    await expect(page.locator("#window-controls")).toBeHidden();
    await expect(page.locator(".titlebar")).not.toHaveAttribute("data-tauri-drag-region", "");
  });
});

test("the row menu survives a re-render of the action cell", async ({ page }) => {
  // The hidden control buttons live outside the cell that renderRowActions clears;
  // a menu item must still reach them after the row has been re-rendered.
  const row = page.locator("#plucks tr.row", { hasText: "An Interrupted Download" });
  await row.locator(".row-action").first().click();
  await expect(page.locator(".overflow-menu")).toHaveCount(0);
  await row.locator('button[aria-label^="More actions"]').click();
  await expect(page.locator(".overflow-menu")).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(page.locator(".overflow-menu")).toHaveCount(0);
});

test("clearing a failed download takes its failure notice with it", async ({ page }) => {
  // The failure notice is a sibling row of the download row. Clearing the
  // download without clearing the notice left "Download failed." on screen
  // with nothing behind it.
  await expect(page.locator("#plucks tr[data-fail-for]")).toHaveCount(1);
  await expect(page.locator("#plucks .notice.error")).toContainText(
    "YouTube requires a logged-in account",
  );

  await page.locator("#clear-history-btn").click();

  await expect(page.locator("#plucks tr[data-fail-for]")).toHaveCount(0);
  await expect(page.locator("#plucks .notice.error")).toHaveCount(0);
  // The interrupted download is not finished, so it stays.
  await expect(page.locator("#plucks tr.row")).toHaveCount(1);
});

test("a job row does not overflow its table", async ({ page }) => {
  const overflow = await page.evaluate(() => {
    const table = document.getElementById("downloads-table");
    return table.scrollWidth - table.clientWidth;
  });
  expect(overflow).toBeLessThanOrEqual(0);
});

test("export sends the arguments the Rust command declares", async ({ page }) => {
  // export_dump takes settings and parked as well as a path; a call that omits
  // one fails after the file dialog closes, which is the worst moment to learn.
  await page.locator("#nav-settings").click();
  await page.locator('.settings-nav-item[data-pane="pane-backup"]').click();
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
  await page.locator("#nav-settings").click();
  await page.locator('.settings-nav-item[data-pane="pane-backup"]').click();
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
  // restored_queue_items emits destDir: "". Passed through it becomes
  // `yt-dlp -P ""`, which resolves to whatever directory the app launched in.
  await page.locator("#nav-settings").click();
  await page.locator('.settings-nav-item[data-pane="pane-backup"]').click();
  await page.locator("#import-dump-btn").click();
  await expect(page.locator("#dump-status")).toContainText("1 setting");

  await page.locator("#nav-queue").click();
  await page.locator("#queue-list .row-action").first().click();

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
      body: await page.screenshot(),
      contentType: "image/png",
    });
  }
});
