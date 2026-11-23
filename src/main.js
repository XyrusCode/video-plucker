const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;
const { open, save, ask } = window.__TAURI__.dialog;
const { load } = window.__TAURI__.store;
const { revealItemInDir, openUrl } = window.__TAURI__.opener;
const { downloadDir } = window.__TAURI__.path;

const urlInput = document.getElementById("url-input");
const playlistToggle = document.getElementById("playlist-toggle");
const playlistCheck = document.getElementById("playlist-check");
const analyzeBtn = document.getElementById("analyze-btn");
const analyzeError = document.getElementById("analyze-error");
const duplicateBanner = document.getElementById("duplicate-banner");
const duplicateText = document.getElementById("duplicate-text");
const duplicateForce = document.getElementById("duplicate-force");
const metaCard = document.getElementById("meta-card");
const metaThumb = document.getElementById("meta-thumb");
const metaTitle = document.getElementById("meta-title");
const metaSub = document.getElementById("meta-sub");
const metaEntries = document.getElementById("meta-entries");
const analysisBar = document.getElementById("analysis-bar");
const analysisSummary = document.getElementById("analysis-summary");
const analysisGrid = document.getElementById("analysis-grid");
const pluckAllBtn = document.getElementById("pluck-all-btn");
const qualitySelect = document.getElementById("quality-select");
const cookiesSelect = document.getElementById("cookies-select");
const destDirEl = document.getElementById("dest-dir");
const browseBtn = document.getElementById("browse-btn");
const pluckBtn = document.getElementById("pluck-btn");
const plucksEl = document.getElementById("plucks");
const plucksEmpty = document.getElementById("plucks-empty");
const clearHistoryBtn = document.getElementById("clear-history-btn");

// Cookie manager elements
const cookieBody = document.getElementById("cookie-body");
const cookieNameInput = document.getElementById("cookie-name-input");
const cookieAddBtn = document.getElementById("cookie-add-btn");
const cookieError = document.getElementById("cookie-error");

// Backup & restore
const exportDumpBtn = document.getElementById("export-dump-btn");
const importDumpBtn = document.getElementById("import-dump-btn");
const dumpStatus = document.getElementById("dump-status");

// nav + search view
const navDownload = document.getElementById("nav-download");
const navSearch = document.getElementById("nav-search");
const navQueue = document.getElementById("nav-queue");
const navSaved = document.getElementById("nav-saved");
const navHistory = document.getElementById("nav-history");
const navSettings = document.getElementById("nav-settings");
const railActiveCount = document.getElementById("rail-active-count");

// status bar (DESIGN.md §5)
const statusDot = document.getElementById("status-dot");
const statusEngine = document.getElementById("status-engine");
const statusActivity = document.getElementById("status-activity");
const statusDest = document.getElementById("status-dest");
const statusVersion = document.getElementById("status-version");
const viewDownload = document.getElementById("view-download");
const viewSearch = document.getElementById("view-search");
const viewQueue = document.getElementById("view-queue");
const viewSaved = document.getElementById("view-saved");
const viewHistory = document.getElementById("view-history");
const viewSettings = document.getElementById("view-settings");
const historyList = document.getElementById("history-list");
const historyEmpty = document.getElementById("history-empty");
const historySub = document.getElementById("history-sub");
const historyClearBtn = document.getElementById("history-clear-btn");
const savedDownloadAllBtn = document.getElementById("saved-download-all-btn");
const clearTempBtn = document.getElementById("clear-temp-btn");
const clearTempStatus = document.getElementById("clear-temp-status");
const queueList = document.getElementById("queue-list");
const queueEmpty = document.getElementById("queue-empty");
const queueSub = document.getElementById("queue-sub");
const queueStartAllBtn = document.getElementById("queue-start-all-btn");
const siteSelect = document.getElementById("site-select");
const searchInput = document.getElementById("search-input");
const translationSelect = document.getElementById("translation-select");
const searchBtn = document.getElementById("search-btn");
const searchError = document.getElementById("search-error");
const searchResults = document.getElementById("search-results");
const resultDetail = document.getElementById("result-detail");
const detailPoster = document.getElementById("detail-poster");
const detailTitle = document.getElementById("detail-title");
const detailSub = document.getElementById("detail-sub");
const detailBack = document.getElementById("detail-back");
const streamQuality = document.getElementById("stream-quality");
const streamDownload = document.getElementById("stream-download");
const seriesPicker = document.getElementById("series-picker");
const seasonSelect = document.getElementById("season-select");
const epRange = document.getElementById("ep-range");
const epSelectAll = document.getElementById("ep-select-all");
const epCount = document.getElementById("ep-count");
const episodeList = document.getElementById("episode-list");

// window controls (Windows-only frameless chrome, DESIGN.md §5)
const windowControls = document.getElementById("window-controls");
const winMinimize = document.getElementById("win-minimize");
const winMaximize = document.getElementById("win-maximize");
const winClose = document.getElementById("win-close");

let store = null;
let plucksStore = null; // persisted pluck records, for resume after a crash
let queueStore = null; // parked "queue for later" items from failed downloads
let destDir = "";
let currentMeta = null;
/** The tiles of the last batch analysis, one per link. */
let batchTiles = [];
/** How many links are analysed at the same time. */
const BATCH_CONCURRENCY = 3;
let nextJobId = 1;
const jobs = new Map(); // jobId -> job state + DOM refs
const jobQueue = []; // jobs waiting for download slots
let activeJobs = new Set(); // currently running jobs (for parallel mode)

// Get max concurrent downloads based on settings.
// A maxConcurrentDownloads of 0 (the "Unlimited" option) means every queued
// job starts immediately; JSON stores cannot hold Infinity, so 0 is the
// sentinel.
function getMaxConcurrentDownloads() {
  if (appSettings.downloadMode !== "parallel") return 1;
  const max = Number(appSettings.maxConcurrentDownloads);
  return max === 0 ? Infinity : max || 3;
}

// search state
let currentResult = null; // the SearchResult being viewed in the detail panel
let currentDetail = null; // its SeriesDetail (series only)

/* ---------- formatting helpers ---------- */

function fmtBytes(n) {
  if (n == null) return "?";
  const units = ["B", "KB", "MB", "GB"];
  let i = 0;
  while (n >= 1024 && i < units.length - 1) {
    n /= 1024;
    i++;
  }
  return `${n.toFixed(n >= 100 || i === 0 ? 0 : 1)} ${units[i]}`;
}

function fmtSpeed(n) {
  return n == null ? "" : `${fmtBytes(n)}/s`;
}

function fmtEta(secs) {
  if (secs == null) return "";
  secs = Math.round(secs);
  const m = Math.floor(secs / 60);
  const s = secs % 60;
  if (m >= 60) return `${Math.floor(m / 60)}h ${m % 60}m`;
  return `${m}:${String(s).padStart(2, "0")}`;
}

function fmtDuration(secs) {
  if (secs == null) return "";
  secs = Math.round(secs);
  const h = Math.floor(secs / 3600);
  const m = Math.floor((secs % 3600) / 60);
  const s = secs % 60;
  return h > 0
    ? `${h}:${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}`
    : `${m}:${String(s).padStart(2, "0")}`;
}

/* ---------- settings + persistence ---------- */

const TERMS_KEY = "video-plucker-terms-accepted";

// Settings structure with defaults
const DEFAULT_SETTINGS = {
  destDir: null, // will be set to downloadDir() if not provided
  quality: "480", // 480p is the default; the user can change it on Pluck or in Settings
  cookiesBrowser: "none",
  downloadMode: "queue", // "queue" or "parallel"
  maxConcurrentDownloads: 3, // 0 = unlimited
  autoUpdate: true,
  nextJobId: 1
};

// Current settings state
let appSettings = { ...DEFAULT_SETTINGS };

async function initSettings() {
  store = await load("settings.json", { autoSave: true });
  plucksStore = await load("plucks.json", { autoSave: true });
  queueStore = await load("queue.json", { autoSave: true });
  
  // Load all settings with defaults
  await loadSettings();

  // v4.16.0 moved the default quality to 480p. One-time migration: an install
  // that still carries the old "best" default is switched over, a choice the
  // user makes from this release on is kept.
  if (appSettings.quality480Migrated !== true) {
    await saveSetting("quality", "480");
    await saveSetting("quality480Migrated", true);
  }

  // Apply loaded settings to UI
  destDir = appSettings.destDir || (await downloadDir());
  if (appSettings.destDir !== destDir) {
    await saveSetting("destDir", destDir);
  }
  
  qualitySelect.value = appSettings.quality;
  cookiesSelect.value = appSettings.cookiesBrowser;
  nextJobId = appSettings.nextJobId;
  
  renderDestDir();
  await restoreInterruptedPlucks();
  renderPlucksEmptyState();
  await renderQueue();
  await renderHistory();
  await renderCookies();
}

async function loadSettings() {
  for (const [key, defaultValue] of Object.entries(DEFAULT_SETTINGS)) {
    try {
      const saved = await store.get(key);
      appSettings[key] = saved !== null && saved !== undefined ? saved : defaultValue;
    } catch {
      appSettings[key] = defaultValue;
    }
  }
}

async function saveSetting(key, value) {
  appSettings[key] = value;
  await store.set(key, value);
}

async function getSettings() {
  return { ...appSettings };
}

function renderDestDir() {
  destDirEl.textContent = destDir;
  destDirEl.title = destDir;
}

async function saveRecord(rec) {
  await plucksStore.set(String(rec.jobId), rec);
}

async function patchRecord(jobId, patch) {
  const rec = await plucksStore.get(String(jobId));
  if (rec) await plucksStore.set(String(jobId), { ...rec, ...patch });
}

async function removeRecord(jobId) {
  await plucksStore.delete(String(jobId));
}

browseBtn.addEventListener("click", async () => {
  const picked = await open({ directory: true, defaultPath: destDir });
  if (picked) {
    destDir = picked;
    renderDestDir();
    await saveSetting("destDir", destDir);
  }
});

qualitySelect.addEventListener("change", async () => {
  await saveSetting("quality", qualitySelect.value);
  // Keep the Settings pane's select in sync (declared further down; the
  // handler only ever runs after the whole script has loaded).
  const settingsSelect = document.getElementById("settings-quality-select");
  if (settingsSelect) settingsSelect.value = qualitySelect.value;
});

cookiesSelect.addEventListener("change", async () => {
  await saveSetting("cookiesBrowser", cookiesSelect.value);
});

function cookiesFromBrowser() {
  const v = cookiesSelect.value;
  if (!v || v === "none") return null;
  return v;
}

/* ---------- backup & restore ---------- */

function setDumpStatus(msg) {
  dumpStatus.textContent = msg || "";
  dumpStatus.classList.toggle("hidden", !msg);
}

/**
 * Writes a dump whose shape the Android app reads unchanged.
 *
 * The filename mirrors SettingsDump.suggestedFileName so a file that lands in
 * a downloads folder is self-describing whichever app it came from.
 */
async function exportDump() {
  setDumpStatus("");
  exportDumpBtn.disabled = true;
  try {
    const version = appVersionSpan.textContent || "backup";
    const suggested = `video-plucker-dump-${version}-${Date.now()}.json`;
    const target = await save({
      defaultPath: suggested,
      filters: [{ name: "Video Plucker dump", extensions: ["json"] }],
    });
    if (!target) return;
    const parked = await loadQueueEntries();
    const counts = await invoke("export_dump", {
      path: target,
      settings: appSettings,
      parked,
    });
    const skipped = counts.skippedQueueItems
      ? ` ${counts.skippedQueueItems} streaming-site item(s) were left out; they cannot be replayed as a URL.`
      : "";
    setDumpStatus(
      `Exported ${counts.prefsKeys} setting(s), ${counts.cookieFiles} cookie set(s) and ${counts.queueItems} queued item(s).${skipped}`,
    );
  } catch (err) {
    setDumpStatus(`Export failed: ${err}`);
  } finally {
    exportDumpBtn.disabled = false;
  }
}

/**
 * Imports a dump produced by either app.
 *
 * Two things the Rust side cannot do for us. Cookie files are written by the command
 * because they live outside the settings store, but the settings store itself is a
 * frontend store, so the returned prefs have to be written back through it here. And
 * parked items are merged rather than replaced: the Android app replaces its queue on
 * import because it owns the whole list, but doing that here would silently empty
 * someone's downloads the moment they picked a file.
 */
async function importDump() {
  setDumpStatus("");
  importDumpBtn.disabled = true;
  try {
    const picked = await open({
      multiple: false,
      filters: [{ name: "Video Plucker dump", extensions: ["json"] }],
    });
    if (!picked) return;

    const result = await invoke("import_dump", { path: picked });
    for (const [key, value] of Object.entries(result.prefs ?? {})) {
      await saveSetting(key, value);
    }
    for (const item of result.queue ?? []) {
      await queueStore.set(String(item.jobId), item);
    }

    await loadSettings();
    qualitySelect.value = appSettings.quality;
    cookiesSelect.value = appSettings.cookiesBrowser;
    await renderQueue();
    await renderCookies();

    const from = result.appVersion ? ` (from ${result.appVersion})` : "";
    setDumpStatus(
      `Restored ${result.prefsKeys} setting(s), ${result.cookieFiles} cookie set(s), ${result.queueItems} queued item(s) and ${result.ledgerEntries} download record(s)${from}. Restart to pick up the settings change.`,
    );
  } catch (err) {
    setDumpStatus(`Import failed: ${err}`);
  } finally {
    importDumpBtn.disabled = false;
  }
}

/* ---------- cookie manager ---------- */

/** Cookie failures render next to the cookie list, not in the Pluck tab's error line. */
function setCookieError(msg) {
  cookieError.textContent = msg;
  cookieError.classList.toggle("hidden", !msg);
}

async function renderCookies() {
  let entries;
  try {
    entries = await invoke("list_cookies");
  } catch {
    return;
  }
  cookieBody.querySelectorAll(".cookie-row").forEach((r) => r.remove());
  for (const { name } of entries) {
    const row = document.createElement("div");
    row.className = "cookie-row";
    const label = document.createElement("span");
    label.className = "cookie-name";
    label.title = `${name} cookies — matches URLs containing "${name}"`;
    label.textContent = name;
    const status = document.createElement("span");
    status.className = "cookie-status";
    status.textContent = "Saved";
    const clear = document.createElement("button");
    clear.className = "quiet danger";
    clear.textContent = "Clear";
    clear.addEventListener("click", async () => {
      setCookieError("");
      try {
        await invoke("delete_cookie", { name });
      } catch (err) {
        setCookieError(`Failed to clear ${name} cookies: ${err}`);
      }
      await renderCookies();
    });
    row.append(label, status, clear);
    cookieBody.appendChild(row);
  }
  if (!entries.length) {
    const empty = document.createElement("p");
    empty.className = "cookie-empty";
    empty.textContent = "No cookie sets saved yet — add one above.";
    cookieBody.appendChild(empty);
  }
}

async function importCookie() {
  const name = cookieNameInput.value.trim();
  setCookieError("");
  if (!name) {
    setCookieError("Type a name for this cookie set first (e.g. insta).");
    return;
  }
  const selected = await open({
    multiple: false,
    filters: [{ name: "Cookies", extensions: ["txt"] }],
  });
  if (!selected) return;
  try {
    const savedAs = await invoke("import_cookie", { name, sourcePath: selected });
    cookieNameInput.value = "";
    await renderCookies();
    // A profile name is sanitised before it reaches disk, so "VK Video" lands as "vk_video".
    // Saying so beats a row that appears under a name the user did not type.
    if (savedAs !== name) setCookieError(`Saved as "${savedAs}".`);
  } catch (err) {
    setCookieError(`Failed to import "${name}" cookies: ${err}`);
  }
}

cookieAddBtn.addEventListener("click", importCookie);
cookieNameInput.addEventListener("keydown", (e) => {
  if (e.key === "Enter") importCookie();
});

/* ---------- URL helpers ---------- */

function cleanPlatformUrl(url) {
  // Strip tracking query params from TikTok video URLs.
  if (/tiktok\.com\/.*\/video\//i.test(url)) {
    return url.split("?")[0];
  }
  return url;
}

/* ---------- analyze ---------- */

function isPlaylistUrl(url) {
  return url.includes("list=");
}

/**
 * The links in the field, one per line.
 *
 * A pasted line often holds several links, and a URL never contains a
 * space, so whitespace is the separator.
 */
function inputUrls() {
  return urlInput.value.split(/\s+/).map((u) => u.trim()).filter(Boolean);
}

urlInput.addEventListener("input", () => {
  updateInputMode();
  pluckBtn.disabled = true;
  currentMeta = null;
});

/**
 * One link is analysed into the detail card, several into the grid.
 *
 * The grid plucks tile by tile and through "Pluck all", so the single
 * Pluck button has nothing to pluck and is not shown.
 */
function updateInputMode() {
  pluckBtn.classList.toggle("hidden", inputUrls().length > 1);
  playlistToggle.classList.toggle("hidden", !isPlaylistUrl(urlInput.value));
}

function playlistMode() {
  return isPlaylistUrl(urlInput.value.trim()) && playlistCheck.checked;
}

/** The field is empty: the link has moved on to the downloads list. */
function clearInput() {
  urlInput.value = "";
  updateInputMode();
  pluckBtn.disabled = true;
  currentMeta = null;
  metaCard.classList.add("hidden");
  // The duplicate notice belongs to the video that was just handed off. Leaving it up would
  // claim the next video was already downloaded.
  hideDuplicate();
}

/** The message an analysis failure shows, wherever it is shown. */
function analyzeErrorText(err) {
  let msg = String(err);
  // YouTube's bot check is only escapable with browser cookies — point the
  // user at the control that does it.
  if (/not a bot|Sign in to confirm/i.test(msg) && !cookiesFromBrowser()) {
    msg += "\n\nTip: set “YouTube login” to your browser to use its cookies.";
  }
  return msg;
}

async function analyze() {
  const urls = inputUrls();
  if (!urls.length) return;

  analyzeError.classList.add("hidden");
  analyzeBtn.disabled = true;
  analyzeBtn.textContent = "Analyzing…";
  pluckBtn.disabled = true;
  try {
    if (urls.length === 1) {
      await analyzeOne(urls[0]);
    } else {
      await analyzeMany(urls);
    }
  } finally {
    analyzeBtn.disabled = false;
    analyzeBtn.textContent = "Analyze";
  }
}

/**
 * One link: the result is the detail card under the field.
 *
 * A single link is the common case, so it keeps the detail card. Several
 * links become a grid instead, because a card per link would fill the
 * window with cards and hide what each one is.
 */
async function analyzeOne(url) {
  hideBatch();
  hideDuplicate();
  currentMeta = null;
  updateInputMode();
  const clean = cleanPlatformUrl(url);
  if (clean !== url) urlInput.value = clean;
  try {
    currentMeta = await invoke("fetch_metadata", {
      url: clean,
      playlistMode: playlistMode(),
      cookiesFromBrowser: cookiesFromBrowser(),
    });
    currentMeta.duplicate = currentMeta.id ? await findDuplicate(currentMeta.id) : null;
    renderMeta(currentMeta);
    renderDuplicate(currentMeta.duplicate);
    pluckBtn.disabled = false;
  } catch (err) {
    metaCard.classList.add("hidden");
    analyzeError.textContent = analyzeErrorText(err);
    analyzeError.classList.remove("hidden");
  }
}

/**
 * Several links: one tile per link, analysed a few at a time.
 *
 * A tile is self-contained. It carries the link and what was read about
 * it, so editing the field afterwards does not change what a tile plucks.
 */
async function analyzeMany(urls) {
  hideDuplicate();
  currentMeta = null;
  metaCard.classList.add("hidden");
  updateInputMode();
  batchTiles = urls.map((url) => createTile(url));
  analysisGrid.innerHTML = "";
  for (const tile of batchTiles) {
    analysisGrid.appendChild(tile.el);
    paintTile(tile);
  }
  analysisGrid.classList.remove("hidden");
  analysisBar.classList.remove("hidden");
  renderBatchBar();

  // A few at a time: twenty yt-dlp runs at once would hammer the machine
  // and the site.
  const pending = batchTiles.slice();
  const workers = Array.from(
    { length: Math.min(BATCH_CONCURRENCY, pending.length) },
    async () => {
      while (pending.length) {
        const tile = pending.shift();
        // The batch was replaced while this link was waiting.
        if (!batchTiles.includes(tile)) continue;
        await analyzeTile(tile);
      }
    },
  );
  await Promise.all(workers);
}

/** One tile's analysis. A failure lands on the tile, not on the window. */
async function analyzeTile(tile) {
  tile.state = "analysing";
  tile.error = null;
  tile.thumbFailed = false;
  paintTile(tile);
  renderBatchBar();
  try {
    const meta = await invoke("fetch_metadata", {
      url: tile.url,
      playlistMode: isPlaylistUrl(tile.url) && playlistCheck.checked,
      cookiesFromBrowser: cookiesFromBrowser(),
    });
    meta.duplicate = meta.id ? await findDuplicate(meta.id) : null;
    tile.meta = meta;
    tile.state = "ready";
  } catch (err) {
    tile.meta = null;
    tile.error = analyzeErrorText(err);
    tile.state = "failed";
  }
  paintTile(tile);
  renderBatchBar();
}

function createTile(url) {
  const el = document.createElement("div");
  el.className = "tile";
  el.innerHTML = `
    <div class="tile-thumb">
      <img class="tile-img" alt="" />
      <div class="tile-thumb-error hidden">
        <span class="tile-thumb-note"></span>
        <button class="tile-refetch">Refetch</button>
      </div>
    </div>
    <div class="tile-body">
      <span class="tile-title"></span>
      <span class="tile-sub"></span>
      <div class="tile-foot">
        <button class="tile-pluck">Pluck</button>
      </div>
    </div>
  `;
  const tile = {
    url,
    state: "analysing",
    meta: null,
    error: null,
    thumbFailed: false,
    el,
    img: el.querySelector(".tile-img"),
    errorBox: el.querySelector(".tile-thumb-error"),
    errorNote: el.querySelector(".tile-thumb-note"),
    title: el.querySelector(".tile-title"),
    sub: el.querySelector(".tile-sub"),
    pluckBtn: el.querySelector(".tile-pluck"),
    refetchBtn: el.querySelector(".tile-refetch"),
  };
  // A thumbnail that did not load says so on its own tile, and the refetch
  // button is right there on it.
  tile.img.addEventListener("error", () => {
    tile.thumbFailed = true;
    paintTile(tile);
  });
  tile.pluckBtn.addEventListener("click", () => pluckTile(tile));
  tile.refetchBtn.addEventListener("click", () => analyzeTile(tile));
  return tile;
}

function paintTile(tile) {
  const meta = tile.meta;
  tile.el.dataset.state = tile.state;
  if (tile.state === "ready" && meta) {
    tile.title.textContent = meta.title;
    tile.sub.textContent = [
      meta.duplicate ? "Already downloaded" : "",
      prettySource(meta.source),
      fmtDuration(meta.duration),
    ]
      .filter(Boolean)
      .join(" · ");
    tile.pluckBtn.disabled = false;
    if (meta.thumbnail && !tile.thumbFailed) {
      tile.img.src = meta.thumbnail;
      tile.img.classList.remove("hidden");
      tile.errorBox.classList.add("hidden");
    } else {
      tile.img.classList.add("hidden");
      tile.errorNote.textContent = tile.thumbFailed
        ? "The thumbnail could not be fetched."
        : "This link has no thumbnail.";
      tile.errorBox.classList.remove("hidden");
    }
  } else if (tile.state === "failed") {
    tile.title.textContent = tile.url;
    tile.sub.textContent = "Could not be analysed";
    tile.img.classList.add("hidden");
    tile.errorNote.textContent = (tile.error || "Could not fetch this").replace(/\s+/g, " ");
    tile.errorBox.classList.remove("hidden");
    tile.pluckBtn.disabled = true;
  } else {
    tile.title.textContent = "Analysing…";
    tile.sub.textContent = tile.url;
    tile.img.classList.add("hidden");
    tile.errorBox.classList.add("hidden");
    tile.pluckBtn.disabled = true;
  }
}

function renderBatchBar() {
  const ready = batchTiles.filter((t) => t.state === "ready").length;
  const failed = batchTiles.filter((t) => t.state === "failed").length;
  const working = batchTiles.length - ready - failed;
  const parts = [];
  if (ready) parts.push(`${ready} ready`);
  if (working) parts.push(`${working} analysing`);
  if (failed) parts.push(`${failed} failed`);
  analysisSummary.textContent = parts.length ? parts.join(" · ") : "Nothing analysed";
  pluckAllBtn.disabled = ready === 0;
  pluckAllBtn.textContent = ready > 1 ? `Pluck all (${ready})` : "Pluck all";
}

/** A tile that has been plucked leaves the grid: it is in the queue now. */
function removeTile(tile) {
  const at = batchTiles.indexOf(tile);
  if (at !== -1) batchTiles.splice(at, 1);
  tile.el.remove();
  renderBatchBar();
  if (!batchTiles.length) hideBatch();
}

function hideBatch() {
  batchTiles = [];
  analysisGrid.innerHTML = "";
  analysisGrid.classList.add("hidden");
  analysisBar.classList.add("hidden");
  analysisSummary.textContent = "";
}

async function pluckTile(tile) {
  if (!tile.meta) return;
  if (tile.meta.duplicate) {
    const dup = tile.meta.duplicate;
    const when = dup.downloadedAt ? new Date(dup.downloadedAt).toLocaleDateString() : null;
    const again = await ask(
      `“${dup.title}” was already downloaded${when ? ` on ${when}` : ""}. Download it again?`,
      { title: "Already downloaded", okLabel: "Download again", cancelLabel: "Skip" },
    );
    if (!again) {
      tile.sub.textContent = "Already downloaded - not queued";
      return;
    }
  }
  await startPluck(tile.url, tile.meta, { skipDuplicateCheck: true });
  removeTile(tile);
}

/**
 * One prompt for a whole "download all" batch. Re-checks every id against
 * the ledger/dest folder (cheap) and, when anything is already downloaded,
 * asks once whether to skip those instead of asking once per item.
 */
async function bulkDuplicateDecision(ids) {
  const matches = await Promise.all(
    ids.map((id) => (id ? findDuplicate(id) : null)),
  );
  const dupes = matches.filter(Boolean).length;
  if (!dupes) return { matches, skipDupes: false };
  const skip = await ask(
    `${dupes} of these ${dupes === 1 ? "was" : "were"} already downloaded. Skip the ones you already have?`,
    { title: "Already downloaded", okLabel: "Skip those", cancelLabel: "Download all anyway" },
  );
  return { matches, skipDupes: skip };
}

pluckAllBtn.addEventListener("click", async () => {
  const ready = batchTiles.filter((t) => t.state === "ready");
  const { matches, skipDupes } = await bulkDuplicateDecision(ready.map((t) => t.meta?.id || null));
  for (let i = 0; i < ready.length; i++) {
    const tile = ready[i];
    if (skipDupes && matches[i]) {
      tile.sub.textContent = "Already downloaded - not queued";
      continue;
    }
    await startPluck(tile.url, tile.meta, { skipDuplicateCheck: true });
    removeTile(tile);
  }
});

analyzeBtn.addEventListener("click", analyze);
urlInput.addEventListener("keydown", (e) => {
  // The field holds several links, so Enter is a newline. Ctrl+Enter analyses.
  if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
    e.preventDefault();
    analyze();
  }
});
playlistCheck.addEventListener("change", () => {
  pluckBtn.disabled = true;
  currentMeta = null;
});

function prettySource(source) {
  if (!source) return "";
  const s = source.toLowerCase();
  if (s.includes("youtube")) return "YouTube";
  if (s.includes("twitter") || s === "x") return "X (Twitter)";
  return source;
}

function renderMeta(meta) {
  metaThumb.src = meta.thumbnail || "";
  metaThumb.classList.toggle("hidden", !meta.thumbnail);
  metaTitle.textContent = meta.title;

  const label = prettySource(meta.source);
  if (meta.kind === "playlist") {
    metaSub.textContent = `${label ? label + " playlist" : "Playlist"} · ${meta.entryCount} videos`;
    metaEntries.innerHTML = "";
    for (const title of meta.entries.slice(0, 50)) {
      const li = document.createElement("li");
      li.textContent = title;
      metaEntries.appendChild(li);
    }
    if (meta.entryCount > 50) {
      const li = document.createElement("li");
      li.textContent = `… and ${meta.entryCount - 50} more`;
      metaEntries.appendChild(li);
    }
    metaEntries.classList.remove("hidden");
  } else {
    metaSub.textContent = [label, fmtDuration(meta.duration)]
      .filter(Boolean)
      .join(" · ");
    metaEntries.classList.add("hidden");
  }

  // Only offer qualities the video actually has: disable tiers above the max
  // and below the min available height (playlists have unknown heights).
  const heights = meta.heights || [];
  const maxHeight = heights.length ? Math.max(...heights) : null;
  const minHeight = heights.length ? Math.min(...heights) : null;
  for (const opt of qualitySelect.options) {
    const h = parseInt(opt.value, 10);
    opt.disabled =
      meta.kind === "video" &&
      maxHeight != null &&
      !isNaN(h) &&
      (h > maxHeight || h < minHeight);
  }
  if (qualitySelect.selectedOptions[0]?.disabled) qualitySelect.value = "best";

  metaCard.classList.remove("hidden");
}

/* ---------- icons (one stroke weight, drawn SVG; DESIGN.md §7) ---------- */

const ICON_PATHS = {
  play: '<path d="M4.5 3.2v9.6l7.4-4.8-7.4-4.8Z" fill="currentColor"/>',
  pause: '<path d="M4.5 3.5v9M11.5 3.5v9" stroke-linecap="round"/>',
  close: '<path d="m4 4 8 8M12 4l-8 8" stroke-linecap="round"/>',
  folder: '<path d="M2.5 4.5a1 1 0 0 1 1-1h2.7l1.3 1.5h5a1 1 0 0 1 1 1v5.5a1 1 0 0 1-1 1h-9a1 1 0 0 1-1-1v-7Z" stroke-linejoin="round"/>',
  clock: '<circle cx="8" cy="8" r="5.3"/><path d="M8 5v3.3l2 1.4" stroke-linecap="round"/>',
  flag: '<path d="M4 14V2.5M4 3h7.5l-1.5 2.5L11.5 8H4" stroke-linejoin="round"/>',
  trash: '<path d="M3 4.5h10M6.5 4.5V3h3v1.5M4.5 4.5l.6 8.5h5.8l.6-8.5" stroke-linejoin="round"/>',
  retry: '<path d="M12.9 8a4.9 4.9 0 1 1-1.45-3.47" stroke-linecap="round"/><path d="M13.2 2.4v3h-3" stroke-linecap="round" stroke-linejoin="round"/>',
  link: '<path d="M6.67 8.67a3.33 3.33 0 0 0 5.03.36l2-2a3.33 3.33 0 0 0-4.71-4.71l-1.15 1.14" stroke-linecap="round" stroke-linejoin="round"/><path d="M9.33 7.33a3.33 3.33 0 0 0-5.03-.36l-2 2a3.33 3.33 0 0 0 4.71 4.71l1.14-1.14" stroke-linecap="round" stroke-linejoin="round"/>',
  more: '<circle cx="3.5" cy="8" r="1.1" fill="currentColor"/><circle cx="8" cy="8" r="1.1" fill="currentColor"/><circle cx="12.5" cy="8" r="1.1" fill="currentColor"/>',
};

const ICONS = Object.fromEntries(
  Object.entries(ICON_PATHS).map(([name, body]) => [
    name,
    `<svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.4" aria-hidden="true">${body}</svg>`,
  ]),
);

/** Brief visual confirmation that the link is on the clipboard. */
function flashCopied(btn) {
  btn.classList.add("copied");
  const prev = btn.title;
  btn.title = "Copied!";
  clearTimeout(flashCopied._t);
  flashCopied._t = setTimeout(() => {
    btn.classList.remove("copied");
    btn.title = prev || "Copy original link";
  }, 1200);
}

/* ---------- status bar (DESIGN.md §5) ---------- */

// Per-job speeds in bytes/second, summed for the status bar.
const jobSpeeds = new Map();
let engineMessage = null;

function recordSpeed(job, speed) {
  jobSpeeds.set(job.id, speed || 0);
  updateStatusBar();
}

/** Transient engine text ("Updating downloader…"), cleared on the next idle update. */
function setEngine(message) {
  engineMessage = message;
  updateStatusBar();
}

/** A one-off status flash that clears itself, for errors the user must see. */
function flashStatus(message) {
  setEngine(message);
  clearTimeout(flashStatus._timer);
  flashStatus._timer = setTimeout(() => {
    if (engineMessage === message) setEngine(null);
  }, 6000);
}

function updateStatusBar() {
  const all = [...jobs.values()];
  const active = all.filter((j) => j.statusTextEl.textContent === "Plucking" || j.statusTextEl.textContent === "Processing");
  const queued = jobQueue.length;
  const paused = all.filter((j) => j.statusTextEl.textContent === "Paused").length;

  let speed = 0;
  for (const j of active) speed += jobSpeeds.get(j.id) || 0;
  jobSpeeds.forEach((_, id) => {
    if (!jobs.has(id)) jobSpeeds.delete(id);
  });

  const failed = all.some((j) => j.statusEl.className.includes("fail"));
  const busy = Boolean(engineMessage) || active.length > 0;
  statusDot.className = `status-dot${failed ? " error" : busy ? " busy" : ""}`;
  statusEngine.textContent = engineMessage || (failed ? "Attention" : "Ready");

  const parts = [];
  if (active.length) parts.push(`${active.length} active`);
  if (speed > 0) parts.push(`${fmtSpeed(speed)}`);
  if (queued) parts.push(`${queued} queued`);
  if (paused) parts.push(`${paused} paused`);
  statusActivity.textContent = parts.length ? parts.join(" · ") : "Idle";

  statusDest.textContent = destDir || "";
  statusDest.title = destDir || "";
  statusVersion.textContent = appVersionSpan?.textContent && appVersionSpan.textContent !== "Loading…"
    ? `v${appVersionSpan.textContent}`
    : "";
  renderPlucksEmptyState();
}

/* ---------- plucks ---------- */

// A "params" object fully describes a pluck: { url, quality, destDir,
// playlistMode, title, itemCount }. `titles` is an optional per-item title list
// (only available right after analyze, not after a restart).
function createJobCard(jobId, params, { completed = 0, titles = [] } = {}) {
  const isPlaylist = params.playlistMode;
  const itemCount = params.itemCount || 1;

  // One row per download, and one extra row per playlist item inserted
  // directly beneath it (DESIGN.md §6.2). The item rows live in the same
  // tbody so expansion is a DOM insertion, not a second container.
  const card = document.createElement("tr");
  card.className = "row";
  card.innerHTML = `
    <td>
      <span class="cell-name">
        <button class="cell-chevron hidden" aria-expanded="false" aria-label="Show items">
          <svg viewBox="0 0 10 10" fill="none" stroke="currentColor" stroke-width="1.3" aria-hidden="true">
            <path d="M3.5 1.5 7 5l-3.5 3.5" stroke-linecap="round" stroke-linejoin="round" />
          </svg>
        </button>
        <span class="name"></span>
      </span>
    </td>
    <td class="col-format cell-format"></td>
    <td class="col-size num cell-size"></td>
    <td>
      <span class="progress-cell">
        <span class="bar"><span class="bar-fill"></span></span>
        <span class="progress-pct">0%</span>
      </span>
    </td>
    <td><span class="cell-status"><span class="dot"></span><span class="status-text">Starting…</span></span></td>
    <td><span class="cell-actions"></span></td>
  `;
  card.querySelector(".name").textContent = params.title;
  plucksEl.prepend(card);

  // The status cell keeps the "ok"/"fail"/"paused"/"queued" classes the
  // rest of the file already toggles, so those call sites are unchanged.
  const statusEl = card.querySelector(".cell-status");
  const actionsEl = card.querySelector(".cell-actions");

  const job = {
    id: jobId,
    params,
    isPlaylist,
    itemCount,
    completed,
    activeIndex: 0,
    lastFile: null,
    titles,
    // failure bookkeeping (failure-UX overhaul)
    rawErrors: [], // technical yt-dlp lines, only ever sent in issue reports
    friendlyError: null, // plain-language verdict shown to the user
    failureKind: null, // "login_required" | "stale_engine" | ...
    // The failure notice is a sibling row of the job row, so it has to be
    // removed with it. The job holds the element instead of the DOM being
    // searched for it, so no path that clears a job can leave one behind.
    failRow: null,
    card,
    actionsEl,
    statusTextEl: card.querySelector(".status-text"),
    itemLine: card.querySelector(".cell-name .name"),
    overallFill: card.querySelector(".bar-fill"),
    overallPct: card.querySelector(".progress-pct"),
    sizeEl: card.querySelector(".cell-size"),
    formatEl: card.querySelector(".cell-format"),
    statusEl,
    itemRows: new Map(),
    queued: false,
    // The seven buttons the old card carried as visible chips. They are
    // reachable from the overflow menu now, so they are modelled as
    // descriptors rather than as live DOM nodes.
    menu: null,
  };

  job.formatEl.textContent = params.quality ? qualityLabel(params.quality) : "";

  // Hidden control elements. The event handlers below attach to these
  // exactly as they did when they were visible buttons, so the download
  // lifecycle is untouched; the overflow menu drives them.
  const controlsEl = document.createElement("span");
  controlsEl.hidden = true;
  card.appendChild(controlsEl);
  const makeHiddenButton = (cls) => {
    const b = document.createElement("button");
    b.className = `job-control ${cls} hidden`;
    controlsEl.appendChild(b);
    return b;
  };
  job.cancelBtn = makeHiddenButton("job-cancel");
  job.pauseBtn = makeHiddenButton("job-pause");
  job.resumeBtn = makeHiddenButton("job-resume");
  job.retryBtn = makeHiddenButton("job-retry");
  job.queueBtn = makeHiddenButton("job-queue");
  job.reportBtn = makeHiddenButton("job-report");
  job.openBtn = makeHiddenButton("job-open");
  job.dismissBtn = makeHiddenButton("job-dismiss");
  job.copyLinkBtn = makeHiddenButton("job-copy-link");

  job.expandBtn = card.querySelector(".cell-chevron");
  job.itemsEl = document.createElement("tbody");
  job.itemsEl.className = "item-rows hidden";

  if (isPlaylist) {
    job.expandBtn.classList.remove("hidden");
    job.expandBtn.addEventListener("click", () => toggleItems(job));
  }

  job.cancelBtn.addEventListener("click", async () => {
    job.cancelBtn.disabled = true;
    if (job.queued) {
      removeFromQueue(job);
      finishJob(job, { ok: false, cancelled: true });
      return;
    }
    try {
      await invoke("cancel_pluck", { jobId });
    } catch (err) {
      job.cancelBtn.disabled = false;
      flashStatus(`Could not cancel: ${err}`);
    }
  });

  job.pauseBtn.addEventListener("click", async () => {
    job.pauseBtn.disabled = true;
    try {
      await invoke("pause_pluck", { jobId });
    } catch (err) {
      job.pauseBtn.disabled = false;
      flashStatus(`Could not pause: ${err}`);
    }
  });

  job.resumeBtn.addEventListener("click", () => resumeJob(job));
  job.retryBtn.addEventListener("click", () => retryJob(job));
  job.queueBtn.addEventListener("click", () => queueForLater(job));
  job.reportBtn.addEventListener("click", () => reportIssue(job));
  job.dismissBtn.addEventListener("click", async () => {
    // A live download removed from the list must not keep running behind
    // the user's back: cancel it first, then sweep the row away.
    const live = !job.cancelBtn.classList.contains("hidden");
    if (live) {
      try {
        await invoke("cancel_pluck", { jobId });
      } catch {
        // Already gone or never started — removal proceeds either way.
      }
    }
    await removeRecord(jobId);
    removeFromActiveJobs(job);
    removeFromQueue(job);
    jobs.delete(jobId);
    removeJob(job);
    renderPlucksEmptyState();
    updateStatusBar();
    drainQueue();
  });
  job.openBtn.addEventListener("click", () => {
    if (job.lastFile) revealItemInDir(job.lastFile);
  });
  job.copyLinkBtn.addEventListener("click", async () => {
    if (!job.params.url) return;
    try {
      await navigator.clipboard.writeText(job.params.url);
      flashCopied(job.copyLinkBtn);
    } catch {}
  });
  // The original link is always worth copying, whatever state the job is
  // in — only a streaming-site job has no single URL, and it stays hidden.
  if (job.params.url) job.copyLinkBtn.classList.remove("hidden");

  renderRowActions(job);

  jobs.set(jobId, job);
  return job;
}

/**
 * The action cell (DESIGN.md §6.2).
 *
 * One primary action stays visible because it is the thing the user is
 * most likely to want; everything else lives in the overflow menu, so a
 * resting table shows a name and a status rather than eight buttons.
 */
function renderRowActions(job) {
  job.actionsEl.innerHTML = "";

  const primary = primaryActionFor(job);
  if (primary) job.card.dataset.needsAction = "";
  else delete job.card.dataset.needsAction;
  if (primary) {
    const b = document.createElement("button");
    b.className = "row-action";
    b.type = "button";
    b.setAttribute("aria-label", primary.label);
    b.title = primary.label;
    b.innerHTML = ICONS[primary.icon];
    b.addEventListener("click", primary.run);
    job.actionsEl.appendChild(b);
  }

  const extras = menuActionsFor(job);
  if (!extras.length) return;

  const more = document.createElement("button");
  more.className = "row-action";
  more.type = "button";
  more.setAttribute("aria-label", `More actions for ${job.params.title || "this download"}`);
  more.title = "More actions";
  more.innerHTML = ICONS.more;
  more.addEventListener("click", (e) => {
    e.stopPropagation();
    openMenu(more, extras);
  });
  job.actionsEl.appendChild(more);
}

/** The single action a row offers outside the menu, or null. */
function primaryActionFor(job) {
  const shown = (btn) => !btn.classList.contains("hidden");
  if (shown(job.resumeBtn)) return { label: "Resume", icon: "play", run: () => resumeJob(job) };
  if (shown(job.retryBtn)) return { label: "Retry", icon: "retry", run: () => job.retryBtn.click() };
  if (shown(job.pauseBtn)) return { label: "Pause", icon: "pause", run: () => job.pauseBtn.click() };
  if (shown(job.cancelBtn)) return { label: "Cancel", icon: "close", run: () => job.cancelBtn.click() };
  if (shown(job.openBtn)) {
    return {
      label: "Reveal in folder",
      icon: "folder",
      run: () => {
        if (job.lastFile) revealItemInDir(job.lastFile);
      },
    };
  }
  return null;
}

function menuActionsFor(job) {
  const out = [];
  const add = (btn, label, icon, danger = false) => {
    if (btn.classList.contains("hidden")) return;
    out.push({
      label,
      icon,
      danger,
      run: () => {
        closeMenu();
        btn.click();
      },
    });
  };
  add(job.pauseBtn, "Pause", "pause");
  add(job.resumeBtn, "Resume", "play");
  add(job.retryBtn, "Retry", "retry");
  add(job.cancelBtn, "Cancel", "close");
  add(job.queueBtn, "Park for later", "clock");
  add(job.openBtn, "Reveal in folder", "folder");
  add(job.copyLinkBtn, "Copy original link", "link");
  add(job.reportBtn, "Report a problem", "flag");
  add(job.dismissBtn, "Remove from list", "trash", true);
  return out;
}

/* ---------- §6.3 the overflow menu ---------- */

let openMenuEl = null;
let openMenuTrigger = null;

function openMenu(anchor, actions) {
  closeMenu();
  const menu = document.createElement("div");
  menu.className = "overflow-menu";
  menu.setAttribute("role", "menu");
  for (const action of actions) {
    const b = document.createElement("button");
    b.type = "button";
    b.setAttribute("role", "menuitem");
    if (action.danger) b.className = "danger";
    b.innerHTML = `${ICONS[action.icon]}<span>${action.label}</span>`;
    b.addEventListener("click", action.run);
    menu.appendChild(b);
  }
  document.body.appendChild(menu);

  // Flip rather than clip when the menu would leave the viewport.
  const r = anchor.getBoundingClientRect();
  const m = menu.getBoundingClientRect();
  const left = Math.min(r.right - m.width, window.innerWidth - m.width - 8);
  menu.style.left = `${Math.max(8, left)}px`;
  menu.style.top = `${r.bottom + 4}px`;

  openMenuEl = menu;
  openMenuTrigger = anchor;
  anchor.setAttribute("aria-expanded", "true");
  menu.querySelector("button")?.focus();
}

function closeMenu() {
  if (!openMenuEl) return;
  openMenuEl.remove();
  openMenuEl = null;
  openMenuTrigger?.setAttribute("aria-expanded", "false");
  openMenuTrigger = null;
}

document.addEventListener("click", (e) => {
  if (openMenuEl && !openMenuEl.contains(e.target)) closeMenu();
});

document.addEventListener("keydown", (e) => {
  if (e.key === "Escape" && openMenuEl) {
    const trigger = openMenuTrigger;
    closeMenu();
    trigger?.focus();
  }
});

/** Insert or remove the playlist item rows directly beneath their parent. */
function toggleItems(job) {
  const open = job.itemsEl.classList.contains("hidden");
  if (open) {
    buildItemRows(job);
    job.card.after(job.itemsEl);
  }
  job.itemsEl.classList.toggle("hidden", !open);
  job.expandBtn.setAttribute("aria-expanded", String(open));
}

function removeJob(job) {
  job.itemsEl.remove();
  job.card.remove();
  // The failure notice is a sibling of the job row. Removing the row without
  // it left the "Download failed." box on screen after the download was gone.
  if (job.failRow) {
    job.failRow.remove();
    job.failRow = null;
  }
}

function buildItemRows(job) {
  job.itemsEl.innerHTML = "";
  job.itemRows.clear();
  for (let i = 1; i <= job.itemCount; i++) {
    const tr = document.createElement("tr");
    tr.className = "item-row";
    tr.dataset.state = "queued";
    tr.innerHTML = `
      <td><span class="item-title"></span></td>
      <td class="col-format"></td>
      <td class="col-size num"></td>
      <td><span class="bar"><span class="bar-fill warn"></span></span></td>
      <td><span class="cell-status"><span class="dot"></span><span class="state-text">Queued</span></span></td>
      <td></td>`;
    tr.querySelector(".item-title").textContent =
      `${i}. ${job.titles[i - 1] || `Item ${i}`}`;
    job.itemsEl.appendChild(tr);
    job.itemRows.set(i, {
      tr,
      title: tr.querySelector(".item-title"),
      state: tr.querySelector(".state-text"),
      status: tr.querySelector(".cell-status"),
      fill: tr.querySelector(".bar-fill"),
    });
  }
  // when resuming, items already recorded in the archive are done
  for (let i = 1; i <= job.completed; i++) markItem(job, i, "done");
}

function markItem(job, index, state, title) {
  const row = job.itemRows.get(index);
  if (!row) return;
  if (title) row.title.textContent = `${index}. ${title}`;
  row.tr.dataset.state = state;
  if (state === "done") {
    row.state.textContent = "Done";
    row.status.className = "cell-status ok";
    row.fill.style.width = "100%";
    row.fill.className = "bar-fill ok";
  } else if (state === "active") {
    row.state.textContent = "Plucking";
    row.status.className = "cell-status paused";
    row.tr.scrollIntoView({ block: "nearest" });
  } else if (state === "failed") {
    row.state.textContent = "Failed";
    row.status.className = "cell-status fail";
  }
}

// Start (or resume) a pluck. On resume we reuse the same jobId so its yt-dlp
// download archive lines up and finished items are skipped.
async function beginPluck(job, { fresh }) {
  job.queued = false;
  job.card.classList.remove("done");
  job.statusEl.className = "cell-status";
  job.statusTextEl.textContent = "Starting";
  job.cancelBtn.classList.remove("hidden");
  job.cancelBtn.disabled = false;
  job.pauseBtn.classList.remove("hidden");
  job.pauseBtn.disabled = false;
  job.resumeBtn.classList.add("hidden");
  job.retryBtn.classList.add("hidden");
  job.queueBtn.classList.add("hidden");
  job.reportBtn.classList.add("hidden");
  job.openBtn.classList.add("hidden");
  // "Remove from list" stays available for the whole life of the row.
  job.dismissBtn.classList.remove("hidden");
  const failmsg = job.failRow;
  if (failmsg) {
    failmsg.remove();
    job.failRow = null;
  }
  renderRowActions(job);

  if (fresh) {
    await saveRecord({ jobId: job.id, ...job.params, completed: 0, status: "active" });
  } else {
    await patchRecord(job.id, { status: "active" });
  }

  try {
    if (job.params.kind === "stream") {
      await invoke("start_stream_pluck", {
        jobId: job.id,
        site: job.params.site,
        showId: job.params.showId,
        title: job.params.title,
        episodes: job.params.episodes,
        translation: job.params.translation,
        quality: job.params.quality,
        destDir: job.params.destDir,
      });
    } else {
      await invoke("start_pluck", {
        jobId: job.id,
        url: job.params.url,
        quality: job.params.quality,
        destDir: job.params.destDir,
        playlistMode: job.params.playlistMode,
        cookiesFromBrowser: job.params.cookiesFromBrowser ?? null,
        title: job.params.title || null,
        source: job.params.source || null,
      });
    }
    job.statusTextEl.textContent = "Plucking";
    renderRowActions(job);
  } catch (err) {
    finishJob(job, { ok: false, cancelled: false, error: String(err) });
  }
}

async function resumeJob(job) {
  job.completed = 0; // the archive is the source of truth; rebuild from events
  if (job.isPlaylist) buildItemRows(job);
  // A paused job released its slot, so it goes back through the FIFO like any
  // new job; parallel mode starts it immediately.
  if (activeJobs.has(job.id)) {
    await beginPluck(job, { fresh: false });
  } else {
    await enqueueJob(job, { fresh: false });
  }
}

/**
 * Run a failed download again.
 *
 * A retry is a resume with a clean error state. The yt-dlp archive is kept,
 * so items that already finished are not downloaded a second time, and a
 * cookies.txt imported since the failure is picked up by the backend.
 */
async function retryJob(job) {
  job.rawErrors = [];
  job.friendlyError = null;
  job.failureKind = null;
  job.overallFill.style.width = "0%";
  job.overallPct.textContent = "0%";
  job.sizeEl.textContent = "";
  // Hidden before the job is queued, so a second press cannot queue it twice.
  job.retryBtn.classList.add("hidden");
  renderRowActions(job);
  await resumeJob(job);
}

// Add a job to the single-active FIFO queue and start it if the slot is free.
async function enqueueJob(job, { fresh }) {
  job.queued = true;
  jobQueue.push({ job, fresh });
  job.statusTextEl.textContent = "Queued";
  job.statusEl.className = "cell-status queued";
  job.pauseBtn.classList.add("hidden");
  job.cancelBtn.classList.remove("hidden");
  job.cancelBtn.disabled = false;
  renderRowActions(job);
  drainQueue();
}

function drainQueue() {
  const maxConcurrent = getMaxConcurrentDownloads();

  // Start as many jobs as we can within our limit. The set holds job ids,
  // so jobs reattached after a webview teardown count against the limit too.
  while (activeJobs.size < maxConcurrent && jobQueue.length > 0) {
    const entry = jobQueue.shift();
    if (!entry) break;

    activeJobs.add(entry.job.id);
    beginPluck(entry.job, { fresh: entry.fresh });
  }
}

function removeFromActiveJobs(job) {
  activeJobs.delete(job.id);
  drainQueue(); // Try to start more jobs
}

function removeFromQueue(job) {
  const idx = jobQueue.findIndex((e) => e.job === job);
  if (idx !== -1) jobQueue.splice(idx, 1);
  job.queued = false;
}

/**
 * Start a download for a link that has been analysed.
 *
 * The single card and every tile in a batch come through here, so a link
 * behaves the same however it was read. The duplicate check is hoisted here
 * (unless the caller already did its own, or the user explicitly re-downloads
 * from History), so even a deep link or a parked item cannot slip past the
 * warning. A bulk prompt is the caller's job: pass skipDuplicateCheck after
 * asking once for the whole batch.
 */
async function startPluck(url, meta, { skipDuplicateCheck = false } = {}) {
  if (!skipDuplicateCheck && meta && meta.id) {
    const dup = await findDuplicate(meta.id);
    if (dup) {
      const when = dup.downloadedAt ? new Date(dup.downloadedAt).toLocaleDateString() : null;
      const again = await ask(
        `“${dup.title}” was already downloaded${when ? ` on ${when}` : ""}. Download it again?`,
        { title: "Already downloaded", okLabel: "Download again", cancelLabel: "Skip" },
      );
      if (!again) return;
    }
  }
  const isPlaylist = meta.kind === "playlist";
  const params = {
    url,
    quality: qualitySelect.value,
    destDir,
    playlistMode: isPlaylist,
    title: meta.title,
    itemCount: isPlaylist ? meta.entryCount : 1,
    cookiesFromBrowser: cookiesFromBrowser(),
    source: meta.source || null,
    // The video id travels with the job so a parked item can be duplicate-checked when it
    // starts. Null for a playlist, which has no single id.
    videoId: meta.id || null,
  };
  const jobId = nextJobId++;
  await saveSetting("nextJobId", nextJobId);
  const job = createJobCard(jobId, params, {
    titles: isPlaylist ? meta.entries : [],
  });
  await enqueueJob(job, { fresh: true });
}

pluckBtn.addEventListener("click", async () => {
  if (!currentMeta) return;
  if (currentMeta.duplicate) {
    // The banner is the prompt: nothing is queued until the user picks
    // Download anyway. The hoisted check inside startPluck covers every
    // other path (saved tiles, queue, deep links, history).
    analyzeError.textContent = "Skipped: already downloaded. Use Download anyway to fetch it again.";
    analyzeError.classList.remove("hidden");
    return;
  }
  await startPluck(cleanPlatformUrl(urlInput.value.trim()), currentMeta);
  // The link is in the queue now, so the field is empty again.
  clearInput();
});

duplicateForce.addEventListener("click", async () => {
  if (!currentMeta) return;
  hideDuplicate();
  await startPluck(cleanPlatformUrl(urlInput.value.trim()), currentMeta, {
    skipDuplicateCheck: true,
  });
  clearInput();
});

/**
 * The ledger and the destination folder are checked for this video's id. A failed lookup
 * is treated as "not a duplicate": a broken check must never block a download.
 */
async function findDuplicate(id) {
  try {
    return await invoke("find_duplicate", { id, destDir });
  } catch {
    return null;
  }
}

function renderDuplicate(match) {
  duplicateBanner.classList.toggle("hidden", !match);
  if (match) duplicateText.textContent = `Already downloaded: ${match.title}`;
}

function hideDuplicate() {
  duplicateBanner.classList.add("hidden");
}

// Downloads are recorded by the backend (ledger + history) on the yt-dlp
// after_move:DONE line, so this file no longer keeps a copy of the rules.

// Re-create cards for plucks that were still active when the app last closed.
// When the window was torn down to the tray (not a crash), the downloads kept
// running in the backend, so those records reattach to the live job instead
// of offering a Resume that would start the same download twice.
async function restoreInterruptedPlucks() {
  let entries;
  try {
    entries = await plucksStore.entries();
  } catch {
    return;
  }
  let running;
  try {
    running = new Set(await invoke("list_active_jobs"));
  } catch {
    running = new Set();
  }
  for (const [, rec] of entries) {
    if (!rec) continue;
    let params;
    let titles = [];
    if (rec.kind === "stream") {
      if (!rec.episodes) continue;
      params = {
        kind: "stream",
        site: rec.site,
        showId: rec.showId,
        title: rec.title,
        episodes: rec.episodes,
        translation: rec.translation,
        quality: rec.quality,
        destDir: rec.destDir,
        playlistMode: rec.playlistMode,
        itemCount: rec.itemCount,
      };
      titles = rec.episodes.map((e) => e.title || `Episode ${e.episode}`);
    } else {
      if (!rec.url) continue;
      params = {
        url: rec.url,
        quality: rec.quality,
        destDir: rec.destDir,
        playlistMode: rec.playlistMode,
        title: rec.title,
        itemCount: rec.itemCount,
        cookiesFromBrowser: rec.cookiesFromBrowser,
      };
    }
    const job = createJobCard(rec.jobId, params, { completed: rec.completed || 0, titles });
    if (rec.jobId >= nextJobId) {
      nextJobId = rec.jobId + 1;
      await saveSetting("nextJobId", nextJobId);
    }
    job.cancelBtn.classList.add("hidden");
    if (rec.status === "failed") {
      // A failure that outlived a restart keeps its Queue/Report actions.
      job.rawErrors = (rec.rawErrors || "").split("\n").filter(Boolean);
      job.failureKind = rec.failureKind || null;
      job.friendlyError = rec.friendly || null;
      job.statusTextEl.textContent = "Failed";
      job.statusEl.className = "cell-status fail";
      showFailMessage(job, job.friendlyError || "Download failed.");
      job.retryBtn.classList.remove("hidden");
      job.queueBtn.classList.remove("hidden");
      job.reportBtn.classList.remove("hidden");
      job.dismissBtn.classList.remove("hidden");
    } else if (running.has(rec.jobId)) {
      // Still running in the backend after a webview teardown: reattach.
      // The slot counts against the parallel limit, so new jobs queue
      // behind it, and the next pluck event refreshes the card from live.
      activeJobs.add(rec.jobId);
      job.pauseBtn.classList.remove("hidden");
      job.cancelBtn.classList.remove("hidden");
      job.pauseBtn.disabled = false;
      job.cancelBtn.disabled = false;
      job.dismissBtn.classList.remove("hidden");
      job.statusTextEl.textContent = "Plucking";
      job.statusEl.className = "cell-status";
      if (job.isPlaylist) buildItemRows(job);
    } else {
      job.resumeBtn.classList.remove("hidden");
      job.dismissBtn.classList.remove("hidden");
      job.statusTextEl.textContent = "Interrupted, resume to continue";
      job.statusEl.className = "cell-status paused";
    }
    renderRowActions(job);
  }
  updateStatusBar();
}

function setOverall(job, itemFraction) {
  const overall = job.isPlaylist
    ? (job.completed + itemFraction) / job.itemCount
    : itemFraction;
  const pct = Math.min(100, overall * 100);
  job.overallFill.style.width = `${pct.toFixed(1)}%`;
  job.overallPct.textContent = `${Math.round(pct)}%`;
}

function finishJob(job, { ok, cancelled, error }) {
  removeFromActiveJobs(job);
  job.cancelBtn.classList.add("hidden");
  job.pauseBtn.classList.add("hidden");
  if (ok) {
    job.card.classList.add("done");
    job.overallFill.style.width = "100%";
    job.overallFill.className = "bar-fill ok";
    job.overallPct.textContent = "100%";
    job.statusTextEl.textContent = "Completed";
    job.statusEl.className = "cell-status ok";
    if (job.lastFile) job.openBtn.classList.remove("hidden");
    job.dismissBtn.classList.remove("hidden");
    removeRecord(job.id);
  } else if (cancelled) {
    job.statusTextEl.textContent = "Cancelled";
    job.statusEl.className = "cell-status";
    job.dismissBtn.classList.remove("hidden");
    removeRecord(job.id);
  } else {
    // Failed: plain-language verdict only (raw lines are kept for reports),
    // and Retry is the primary action. Parking it for later is still one
    // click away in the overflow menu.
    if (error) appendError(job, error);
    job.statusTextEl.textContent = "Failed";
    job.statusTextEl.title = job.friendlyError || "";
    job.statusEl.className = "cell-status fail";
    showFailMessage(job, job.friendlyError || "Download failed.");
    if (job.lastFile) job.openBtn.classList.remove("hidden");
    job.retryBtn.classList.remove("hidden");
    job.queueBtn.classList.remove("hidden");
    job.reportBtn.classList.remove("hidden");
    job.dismissBtn.classList.remove("hidden");
    patchRecord(job.id, {
      status: "failed",
      completed: job.completed,
      failureKind: job.failureKind,
      friendly: job.friendlyError,
      rawErrors: job.rawErrors.join("\n"),
    });
  }
  renderRowActions(job);
  updateStatusBar();
  drainQueue();
}

function showFailMessage(job, message) {
  if (job.failRow) job.failRow.remove();
  const tr = document.createElement("tr");
  tr.className = "item-row";
  tr.dataset.failFor = job.id;
  const td = document.createElement("td");
  td.colSpan = 6;
  td.innerHTML = `<span class="notice error"></span>`;
  td.firstChild.textContent = message;
  tr.appendChild(td);
  job.card.after(tr);
  job.failRow = tr;
}

function appendError(job, message, friendly = null, kind = null) {
  // Technical errors are collected silently; the user sees the friendly
  // verdict at finish time instead of a raw yt-dlp dump.
  job.rawErrors.push(message);
  if (friendly && (!job.friendlyError || rankFailureKind(kind) > rankFailureKind(job.failureKind))) {
    job.friendlyError = friendly;
    job.failureKind = kind;
  }
}

function rankFailureKind(kind) {
  switch (kind) {
    case "login_required": return 4;
    case "unsupported_content": return 3;
    case "stale_engine": return 2;
    case "network": return 1;
    default: return 0;
  }
}

clearHistoryBtn.addEventListener("click", async () => {
  const toRemove = [];
  for (const [id, job] of jobs) {
    const cls = job.statusEl.className;
    if (cls.includes(" ok") || cls.includes(" fail")) toRemove.push(id);
    else if (job.statusTextEl.textContent === "Cancelled") toRemove.push(id);
  }
  for (const id of toRemove) {
    const job = jobs.get(id);
    if (job) {
      await removeRecord(id);
      removeJob(job);
      jobs.delete(id);
    }
  }
  renderPlucksEmptyState();
  updateStatusBar();
});

/** The Pluck tab's downloads list needs its own empty state: it is no longer a global strip. */
function renderPlucksEmptyState() {
  plucksEmpty.classList.toggle("hidden", jobs.size > 0);
  document.getElementById("downloads-table").classList.toggle("hidden", jobs.size === 0);
  const active = [...jobs.values()].filter((j) => j.statusTextEl.textContent === "Plucking").length;
  railActiveCount.hidden = active === 0;
  railActiveCount.textContent = String(active);
}

/* ---------- backend events ---------- */

listen("pluck://item-start", ({ payload }) => {
  const job = jobs.get(payload.jobId);
  if (!job) return;
  if (job.isPlaylist) {
    if (payload.itemCount > 1 && payload.itemCount !== job.itemCount) {
      job.itemCount = payload.itemCount;
      job.titles = job.titles.slice(0, job.itemCount);
      buildItemRows(job);
    }
    job.activeIndex = payload.itemIndex;
    markItem(job, payload.itemIndex, "active", payload.title);
  }
  job.statusTextEl.textContent = "Plucking";
});

listen("pluck://progress", ({ payload }) => {
  const job = jobs.get(payload.jobId);
  if (!job) return;
  const fraction = payload.percent != null ? payload.percent / 100 : 0;
  if (job.isPlaylist) {
    const idx = payload.itemIndex || job.activeIndex;
    const row = job.itemRows.get(idx);
    if (row) row.fill.style.width = `${(fraction * 100).toFixed(1)}%`;
  }
  setOverall(job, fraction);
  if (payload.total_bytes) job.sizeEl.textContent = fmtBytes(payload.total_bytes);
  recordSpeed(job, payload.speed);
});

listen("pluck://item-done", ({ payload }) => {
  const job = jobs.get(payload.jobId);
  if (!job) return;
  job.lastFile = payload.filepath;
  if (job.isPlaylist) {
    markItem(job, payload.itemIndex, "done");
    job.completed = Math.min(payload.itemIndex, job.itemCount);
    setOverall(job, 0);
    patchRecord(job.id, { completed: job.completed });
  } else {
    job.completed = 1;
    setOverall(job, 1);
    job.statusTextEl.textContent = "Processing";
  }
});

listen("pluck://error", ({ payload }) => {
  const job = jobs.get(payload.jobId);
  if (!job) return;
  if (job.isPlaylist && job.activeIndex) markItem(job, job.activeIndex, "failed");
  appendError(job, payload.message, payload.friendly || null, payload.kind || null);
});

// Transient status text from the backend ("Updating downloader…" during a
// self-heal) — shown without changing the status color class.
listen("pluck://status", ({ payload }) => {
  const job = jobs.get(payload.jobId);
  if (!job) return;
  job.statusEl.className = "cell-status";
  job.statusTextEl.textContent = payload.message;
  setEngine(payload.message);
});

listen("pluck://done", ({ payload }) => {
  const job = jobs.get(payload.jobId);
  if (!job) return;
  // Backend verdict is authoritative when the frontend saw no error lines.
  if (payload.failureKind && !job.failureKind) {
    job.failureKind = payload.failureKind;
    job.friendlyError = payload.friendly || null;
  }
  finishJob(job, { ok: payload.ok, cancelled: payload.cancelled });
  renderHistory();
});

listen("pluck://paused", ({ payload }) => {
  const job = jobs.get(payload.jobId);
  if (!job) return;
  job.statusTextEl.textContent = "Paused";
  job.statusEl.className = "cell-status paused";
  job.pauseBtn.classList.add("hidden");
  job.cancelBtn.classList.add("hidden");
  job.resumeBtn.classList.remove("hidden");
  job.dismissBtn.classList.remove("hidden");
  job.speed = 0;
  // A paused job is not running, so its slot frees up for the next queued
  // download. Resume re-queues through the FIFO.
  removeFromActiveJobs(job);
  patchRecord(job.id, { status: "paused", completed: job.completed });
  renderRowActions(job);
  updateStatusBar();
});

listen("pluck://cookies", ({ payload }) => {
  const job = jobs.get(payload.jobId);
  if (!job) return;
  job.statusTextEl.textContent = `Using cookies: ${payload.file}`;
});

// The window is being hidden to the tray and will be torn down in a moment.
// Stores autosave as they change, but a save in flight at teardown would be
// lost, so they are flushed explicitly while the webview is still alive.
listen("pluck://ui-hiding", async () => {
  await Promise.allSettled([store.save?.(), plucksStore.save?.(), queueStore.save?.()]);
});

/* ---------- queue for later ---------- */
//
// Failed downloads can be parked here instead of retrying immediately: the
// user may be waiting on an app update that fixes whatever broke. Parked
// items never auto-start; starting one goes through the normal FIFO.

async function queueForLater(job) {
  await queueStore.set(String(job.id), {
    jobId: job.id,
    params: job.params,
    titles: job.titles,
    completed: job.completed,
    failureKind: job.failureKind,
    friendly: job.friendlyError,
    rawErrors: job.rawErrors.join("\n"),
    parkedAt: Date.now(),
  });
  removeFromActiveJobs(job);
  removeFromQueue(job);
  jobs.delete(job.id);
  removeJob(job);
  renderPlucksEmptyState();
  updateStatusBar();
  await removeRecord(job.id);
  await renderQueue();
}

async function loadQueueEntries() {
  try {
    return await queueStore.entries();
  } catch {
    return [];
  }
}

/**
 * Parked queue entries, oldest first.
 *
 * Both the list and "Start all" read from here. They used to iterate
 * `entries()` directly while the list sorted by parkedAt, so the batch
 * started in a different order than the one on screen.
 */
async function sortedQueueEntries() {
  const entries = await loadQueueEntries();
  return entries
    .map(([key, item]) => ({ key, item }))
    .sort((a, b) => (a.item?.parkedAt || 0) - (b.item?.parkedAt || 0));
}

async function renderQueue() {
  const sorted = await sortedQueueEntries();
  const count = sorted.filter(({ item }) => item?.params).length;
queueSub.textContent = count
    ? `${count} parked ${count === 1 ? "download" : "downloads"}, oldest first`
    : "Parked downloads waiting for you";
  queueList.innerHTML = "";
  queueEmpty.classList.toggle("hidden", count > 0);
  document.getElementById("queue-table").classList.toggle("hidden", count === 0);
  queueStartAllBtn.classList.toggle("hidden", count === 0);
  for (const { key, item } of sorted) {
    if (!item || !item.params) continue;
    const tr = document.createElement("tr");
    tr.className = "row";
    tr.innerHTML = `
      <td><span class="name"></span></td>
      <td class="col-format"></td>
      <td><span class="cell-status queued"><span class="reason"></span></span></td>
      <td><span class="cell-actions"></span></td>
    `;
    tr.querySelector(".name").textContent = item.params.title || "Untitled download";
    tr.querySelector(".col-format").textContent = item.params.quality
      ? qualityLabel(item.params.quality)
      : "";
    tr.querySelector(".reason").textContent = item.friendly || "Failed earlier";

    const actions = tr.querySelector(".cell-actions");
    const start = document.createElement("button");
    start.className = "row-action";
    start.type = "button";
    start.setAttribute("aria-label", "Start this download");
    start.title = "Start";
    start.innerHTML = ICONS.play;
    start.addEventListener("click", () => startQueued(key, item));

    const remove = document.createElement("button");
    remove.className = "row-action danger";
    remove.type = "button";
    remove.setAttribute("aria-label", "Delete from queue");
    remove.title = "Delete";
    remove.innerHTML = ICONS.trash;
    remove.addEventListener("click", async () => {
      await queueStore.delete(key);
      await renderQueue();
    });

    if (item.params.url) {
      const copy = document.createElement("button");
      copy.className = "row-action";
      copy.type = "button";
      copy.setAttribute("aria-label", "Copy original link");
      copy.title = "Copy original link";
      copy.innerHTML = ICONS.link;
      copy.addEventListener("click", async () => {
        try {
          await navigator.clipboard.writeText(item.params.url);
          flashCopied(copy);
        } catch {}
      });
      actions.append(start, copy, remove);
    } else {
      actions.append(start, remove);
    }
    queueList.appendChild(tr);
  }
  updateStatusBar();
}

function qualityLabel(q) {
  switch (q) {
    case "best": return "Best";
    case "2160": return "2160p";
    case "1440": return "1440p";
    case "1080": return "1080p";
    case "720": return "720p";
    case "480": return "480p";
    default: return q.toUpperCase();
  }
}

async function startQueued(key, item, { skipDuplicateCheck = false } = {}) {
  // A parked item carries the id it was analyzed with, so a video downloaded since it was
  // parked is caught here too. An item from an older backup has no id and is not checked.
  const videoId = item.params?.videoId;
  if (!skipDuplicateCheck && videoId) {
    const duplicate = await findDuplicate(videoId);
    if (duplicate) {
      const when = duplicate.downloadedAt ? new Date(duplicate.downloadedAt).toLocaleDateString() : null;
      // The item stays parked when the user skips: a skip must never eat the
      // only copy of a deliberate save.
      const again = await ask(
        `“${duplicate.title}” was already downloaded${when ? ` on ${when}` : ""}. Download it again?`,
        { title: "Already downloaded", okLabel: "Download again", cancelLabel: "Skip" },
      );
      if (!again) {
        // renderQueue rewrites the sub-line, so the note goes after it.
        await renderQueue();
        queueSub.textContent = "Skipped one: already downloaded.";
        return;
      }
    }
  }
  await queueStore.delete(key);
  const jobId = nextJobId++;
  await saveSetting("nextJobId", nextJobId);
  const job = createJobCard(jobId, {
    ...item.params,
    // A restored item has no destDir: the dump format has nowhere to put one, and the path
    // it would have carried refers to a folder on the other machine. Falling back to the
    // current default is right; passing "" through would become `yt-dlp -P ""`, which
    // resolves to whatever directory the app happened to start in.
    destDir: item.params.destDir || destDir,
  }, {
    completed: 0,
    titles: item.titles || [],
  });
  await enqueueJob(job, { fresh: true });
  await renderQueue();
}

queueStartAllBtn.addEventListener("click", async () => {
  queueStartAllBtn.disabled = true;
  try {
    // Oldest first, the same order the list shows. One prompt covers
    // duplicates across the whole batch.
    const parked = (await sortedQueueEntries()).filter(({ item }) => item && item.params);
    const { matches, skipDupes } = await bulkDuplicateDecision(
      parked.map(({ item }) => item.params?.videoId || null),
    );
    for (let i = 0; i < parked.length; i++) {
      if (skipDupes && matches[i]) continue;
      await startQueued(parked[i].key, parked[i].item, { skipDuplicateCheck: true });
    }
  } finally {
    queueStartAllBtn.disabled = false;
  }
});

/* ---------- history view ---------- */

// Every download this install ever attempted, finished or failed. Kept in
// history.json on the Rust side (see history.rs); this renders it and offers
// the one action that matters: get the file again.

function fmtHistoryDate(ts) {
  if (!ts) return "";
  const d = new Date(ts);
  return d.toLocaleDateString(undefined, { year: "numeric", month: "short", day: "numeric" });
}

async function renderHistory() {
  let entries = [];
  try {
    entries = await invoke("history_entries");
  } catch {
    entries = [];
  }
  entries = entries.slice().reverse(); // newest first
  historySub.textContent = entries.length
    ? `${entries.length} ${entries.length === 1 ? "download" : "downloads"} logged`
    : "Every download, finished or failed";
  historyList.innerHTML = "";
  historyEmpty.classList.toggle("hidden", entries.length > 0);
  document.getElementById("history-table").classList.toggle("hidden", entries.length === 0);
  for (const entry of entries) historyList.appendChild(createHistoryRow(entry));
}

function createHistoryRow(entry) {
  const tr = document.createElement("tr");
  tr.className = "row";
  tr.innerHTML = `
    <td><span class="name"></span></td>
    <td class="col-format"></td>
    <td class="cell-date"></td>
    <td><span class="cell-status"><span class="dot"></span><span class="status-text"></span></span></td>
    <td><span class="cell-actions"></span></td>
  `;
  const nameEl = tr.querySelector(".name");
  nameEl.textContent = entry.title || "Untitled download";
  nameEl.title = entry.fileName || entry.url || entry.title || "";
  tr.querySelector(".col-format").textContent = entry.quality ? qualityLabel(entry.quality) : "";
  tr.querySelector(".cell-date").textContent = fmtHistoryDate(entry.downloadedAt);
  const statusEl = tr.querySelector(".cell-status");
  const statusText = tr.querySelector(".status-text");
  const ok = entry.status === "completed";
  statusEl.className = `cell-status ${ok ? "ok" : "fail"}`;
  statusText.textContent = ok ? "Completed" : "Failed";
  if (!ok && entry.failureKind) statusText.title = entry.failureKind;

  const actions = tr.querySelector(".cell-actions");
  const mk = (icon, label, danger = false) => {
    const b = document.createElement("button");
    b.className = `row-action${danger ? " danger" : ""}`;
    b.type = "button";
    b.setAttribute("aria-label", label);
    b.title = label;
    b.innerHTML = ICONS[icon];
    return b;
  };
  if (entry.url) {
    const redo = mk("retry", "Download this again");
    redo.addEventListener("click", async () => {
      redo.disabled = true;
      await startPluck(
        entry.url,
        {
          kind: entry.playlistMode ? "playlist" : "video",
          title: entry.title,
          entryCount: null,
          entries: [],
          id: entry.videoId || null,
          source: null,
        },
        // Re-downloading from History is explicit: no duplicate prompt.
        { skipDuplicateCheck: true },
      );
      showView("download");
    });
    actions.appendChild(redo);

    const copy = mk("link", "Copy original link");
    copy.addEventListener("click", async () => {
      try {
        await navigator.clipboard.writeText(entry.url);
        flashCopied(copy);
      } catch {}
    });
    actions.appendChild(copy);
  }
  const remove = mk("trash", "Remove from history", true);
  remove.addEventListener("click", async () => {
    try {
      await invoke("history_remove", { id: entry.id });
    } catch (err) {
      flashStatus(`Could not remove the entry: ${err}`);
    }
    await renderHistory();
  });
  actions.appendChild(remove);
  return tr;
}

historyClearBtn.addEventListener("click", async () => {
  const ok = await ask(
    "Remove every entry from the history? Files already on disk are not touched.",
    { title: "Clear history", okLabel: "Clear history", cancelLabel: "Keep" },
  );
  if (!ok) return;
  try {
    await invoke("history_clear");
  } catch (err) {
    flashStatus(`Could not clear the history: ${err}`);
  }
  await renderHistory();
});

/* ---------- issue reporting ---------- */

const ISSUE_REPO = "XyrusCode/video-plucker";

function platformForReport(url) {
  const lower = (url || "").toLowerCase();
  if (lower.includes("youtube.com") || lower.includes("youtu.be")) return "YouTube";
  if (lower.includes("twitter.com") || lower.includes("x.com")) return "X (Twitter)";
  if (lower.includes("tiktok.com")) return "TikTok";
  if (lower.includes("instagram.com")) return "Instagram";
  if (lower.includes("facebook.com")) return "Facebook";
  if (lower.includes("reddit.com")) return "Reddit";
  if (lower.includes("vk.com") || lower.includes("vkvideo.ru")) return "VK";
  if (lower.includes("luciferdonghua")) return "LuciferDonghua";
  if (lower.includes("allanime")) return "AllAnime";
  return "Unknown site";
}

// First 80 chars of an error with non-alphanumerics stripped — the same
// fingerprint the mobile app matches against existing issues.
function errorSignature(rawErrors) {
  const raw = rawErrors[rawErrors.length - 1] || "";
  return raw.replace(/[^a-zA-Z0-9]/g, "").toLowerCase().slice(0, 80);
}

// Look for an open issue already covering this failure so users pile into
// one thread instead of filing duplicate reports.
async function findExistingIssue(rawErrors, platform) {
  const signature = errorSignature(rawErrors);
  const query = encodeURIComponent(
    `repo:${ISSUE_REPO} is:issue is:open "${platform}" in:title`
  );
  try {
    const res = await fetch(
      `https://api.github.com/search/issues?q=${query}&sort=created&per_page=10`,
      { headers: { Accept: "application/vnd.github+json" } }
    );
    if (!res.ok) return null;
    const data = await res.json();
    for (const issue of data.items || []) {
      const body = (issue.body || "").replace(/<[^>]+>/g, "").toLowerCase();
      const title = (issue.title || "").toLowerCase();
      if (
        (signature && body.includes(signature)) ||
        title.includes(platform.toLowerCase())
      ) {
        return issue.html_url;
      }
    }
  } catch {
    // offline / rate-limited: fall through to a fresh issue
  }
  return null;
}

async function reportIssue(job) {
  let version = "";
  try {
    version = await window.__TAURI__.app.getVersion();
  } catch {}

  const platform = platformForReport(job.params.url);
  const body = [
    "**Automated report from Video Plucker**",
    "",
    `- App version: ${version}`,
    `- Platform: ${platform}`,
    `- Quality: ${job.params.quality}`,
    `- Title: ${job.params.title || "(unknown)"}`,
    `- URL: ${job.params.url || "(n/a)"}`,
    "- Progress: playlist items completed before failure are not tracked here",
    "",
    "**Raw error output**",
    "",
    "```",
    ...(job.rawErrors.length ? job.rawErrors.slice(-15) : ["(no captured errors)"]),
    "```",
  ].join("\n");

  const existing = await findExistingIssue(job.rawErrors, platform);
  let url;
  if (existing) {
    url = `${existing}#issuecomment-new`;
  } else {
    const params = new URLSearchParams({
      title: `Download failed — ${platform}`,
      body,
    });
    url = `https://github.com/${ISSUE_REPO}/issues/new?${params}`;
  }
  try {
    openUrl(url);
  } catch (e) {
    console.error("Failed to open issue URL", e);
  }
}

/* ---------- search view ---------- */


/* ---------- saved view ---------- */

const SITE_LABELS = {
  youtube: "YouTube",
  tiktok: "TikTok",
  twitter: "X",
  vk: "VK",
};

const savedSiteSelect = document.getElementById("saved-site-select");
const savedListSelect = document.getElementById("saved-list-select");
const savedListField = document.getElementById("saved-list-field");
const savedSummary = document.getElementById("saved-summary");
const savedError = document.getElementById("saved-error");
const savedGrid = document.getElementById("saved-grid");
const savedEmpty = document.getElementById("saved-empty");
const savedRefreshBtn = document.getElementById("saved-refresh-btn");

let savedItems = [];
let savedConnected = [];
let savedLists = [];
let savedLoaded = false;

/**
 * Read the saved videos of every site the user has a login for.
 *
 * A site without a login is never read and never offered, so the filter
 * cannot lead to an empty list that looks like a bug.
 */
async function loadSaved(force = false) {
  if (savedLoaded && !force) return;
  savedRefreshBtn.disabled = true;
  savedError.classList.add("hidden");
  savedSummary.textContent = "Reading your saved videos…";
  try {
    const listId = savedListSelect.value || "WL";
    const snapshot = await invoke("fetch_saved", { youtubeListId: listId });
    savedItems = snapshot.items || [];
    savedConnected = snapshot.connectedSites || [];
    renderSavedSites();
    await loadSavedLists();
    renderSavedItems();
    const failures = snapshot.failures || {};
    const messages = Object.values(failures);
    if (messages.length) {
      savedError.textContent = messages.join("\n");
      savedError.classList.remove("hidden");
    }
    savedLoaded = true;
  } catch (err) {
    savedError.textContent = String(err);
    savedError.classList.remove("hidden");
  } finally {
    savedRefreshBtn.disabled = false;
  }
}

async function loadSavedLists() {
  if (!savedConnected.includes("youtube")) {
    savedLists = [];
    savedListField.hidden = true;
    return;
  }
  try {
    savedLists = (await invoke("list_youtube_lists")) || [];
  } catch {
    savedLists = [];
  }
  savedListField.hidden = savedLists.length === 0;
  // The list the user picked is kept, so a refresh does not move them back
  // to Watch Later.
  const current = savedListSelect.value;
  savedListSelect.innerHTML = "";
  for (const list of savedLists) {
    const opt = document.createElement("option");
    opt.value = list.id;
    opt.textContent = list.label;
    savedListSelect.appendChild(opt);
  }
  if (savedLists.some((list) => list.id === current)) {
    savedListSelect.value = current;
  }
}

function renderSavedSites() {
  const current = savedSiteSelect.value;
  savedSiteSelect.innerHTML = "";
  const all = document.createElement("option");
  all.value = "";
  all.textContent = "All";
  savedSiteSelect.appendChild(all);
  for (const site of savedConnected) {
    const opt = document.createElement("option");
    opt.value = site;
    opt.textContent = SITE_LABELS[site] || site;
    savedSiteSelect.appendChild(opt);
  }
  savedSiteSelect.value = savedConnected.includes(current) ? current : "";
  savedSiteSelect.disabled = savedConnected.length < 2;
}

function renderSavedItems() {
  const site = savedSiteSelect.value;
  const items = site ? savedItems.filter((i) => i.site === site) : savedItems;
  savedGrid.innerHTML = "";
  for (const item of items) savedGrid.appendChild(createSavedTile(item));
  savedEmpty.classList.toggle("hidden", items.length > 0);
  savedSummary.textContent = items.length
    ? `${items.length} saved ${items.length === 1 ? "video" : "videos"}`
    : "";
}

function createSavedTile(item) {
  const el = document.createElement("div");
  el.className = "tile";
  el.innerHTML = `
    <div class="tile-thumb">
      <img class="tile-img" alt="" />
    </div>
    <div class="tile-body">
      <span class="tile-title"></span>
      <span class="tile-sub"></span>
      <div class="tile-foot">
        <button class="tile-copy" type="button" aria-label="Copy original link" title="Copy original link"></button>
        <button class="tile-pluck">Pluck</button>
      </div>
    </div>
  `;
  const img = el.querySelector(".tile-img");
  if (item.thumbnailUrl) {
    img.src = item.thumbnailUrl;
    img.addEventListener("error", () => img.classList.add("hidden"));
  } else {
    img.classList.add("hidden");
  }
  el.querySelector(".tile-title").textContent = item.title;
  el.querySelector(".tile-sub").textContent = [
    item.author,
    SITE_LABELS[item.site] || item.site,
  ]
    .filter(Boolean)
    .join(" · ");
  const copy = el.querySelector(".tile-copy");
  copy.innerHTML = ICONS.link;
  copy.addEventListener("click", async () => {
    try {
      await navigator.clipboard.writeText(item.url);
      flashCopied(copy);
    } catch {}
  });
  const pluck = el.querySelector(".tile-pluck");
  pluck.addEventListener("click", async () => {
    pluck.disabled = true;
    pluck.textContent = "Plucked";
    await startPluck(item.url, {
      kind: "video",
      title: item.title,
      entryCount: null,
      entries: [],
      id: item.id || null,
      source: item.site || null,
    });
  });
  return el;
}

savedRefreshBtn.addEventListener("click", () => loadSaved(true));
savedSiteSelect.addEventListener("change", renderSavedItems);
savedListSelect.addEventListener("change", () => loadSaved(true));
savedDownloadAllBtn.addEventListener("click", downloadAllSaved);

function visibleSavedItems() {
  const site = savedSiteSelect.value;
  return site ? savedItems.filter((i) => i.site === site) : savedItems;
}

/** Queue every currently visible saved video; one prompt covers duplicates. */
async function downloadAllSaved() {
  const items = visibleSavedItems();
  if (!items.length) return;
  savedDownloadAllBtn.disabled = true;
  savedDownloadAllBtn.textContent = "Starting…";
  try {
    const { matches, skipDupes } = await bulkDuplicateDecision(items.map((i) => i.id || null));
    for (let i = 0; i < items.length; i++) {
      const item = items[i];
      if (skipDupes && matches[i]) continue;
      await startPluck(
        item.url,
        {
          kind: "video",
          title: item.title,
          entryCount: null,
          entries: [],
          id: item.id || null,
          source: item.site || null,
        },
        { skipDuplicateCheck: true },
      );
    }
  } finally {
    savedDownloadAllBtn.disabled = false;
    savedDownloadAllBtn.textContent = "Download all";
  }
}

/* ---------- view switching ---------- */

const views = {
  download: [navDownload, viewDownload],
  queue: [navQueue, viewQueue],
  saved: [navSaved, viewSaved],
  history: [navHistory, viewHistory],
  search: [navSearch, viewSearch],
  settings: [navSettings, viewSettings],
};

function showView(which) {
  for (const [name, [navBtn, viewEl]] of Object.entries(views)) {
    const on = name === which;
    viewEl.classList.toggle("active", on);
    navBtn.classList.toggle("active", on);
    if (on) navBtn.setAttribute("aria-current", "page");
    else navBtn.removeAttribute("aria-current");
  }
}

navDownload.addEventListener("click", () => showView("download"));
navQueue.addEventListener("click", () => showView("queue"));
navSaved.addEventListener("click", () => {
  showView("saved");
  // Read the list once per session, not on every visit. The Refresh button
  // is the way back to the network.
  loadSaved();
});
navHistory.addEventListener("click", () => {
  showView("history");
  // Cheap and always correct: the log only changes when a job terminates.
  renderHistory();
});
navSearch.addEventListener("click", () => showView("search"));
navSettings.addEventListener("click", () => showView("settings"));

async function initSearchView() {
  let sites;
  try {
    sites = await invoke("list_sites");
  } catch {
    sites = [];
  }
  siteSelect.innerHTML = "";
  for (const site of sites) {
    const opt = document.createElement("option");
    opt.value = site.id;
    opt.textContent = site.available ? site.label : `${site.label} (unavailable)`;
    opt.disabled = !site.available;
    siteSelect.appendChild(opt);
  }
  // Select the first available site.
  const firstAvailable = sites.find((s) => s.available);
  if (firstAvailable) siteSelect.value = firstAvailable.id;
}

function translation() {
  return translationSelect.value;
}

async function runSearch() {
  const query = searchInput.value.trim();
  if (!query) return;
  searchError.classList.add("hidden");
  resultDetail.classList.add("hidden");
  searchResults.classList.remove("hidden");
  searchResults.innerHTML = `<p class="results-status">Searching…</p>`;
  searchBtn.disabled = true;
  try {
    const results = await invoke("search_content", {
      site: siteSelect.value,
      query,
      translation: translation(),
    });
    renderResults(results);
  } catch (err) {
    searchResults.classList.add("hidden");
    searchError.textContent = String(err);
    searchError.classList.remove("hidden");
  } finally {
    searchBtn.disabled = false;
  }
}

searchBtn.addEventListener("click", runSearch);
searchInput.addEventListener("keydown", (e) => {
  if (e.key === "Enter") runSearch();
});

function renderResults(results) {
  searchResults.innerHTML = "";
  if (!results.length) {
    searchResults.innerHTML = `<p class="results-status">No results.</p>`;
    return;
  }
  for (const r of results) {
    const card = document.createElement("button");
    card.className = "result-card";
    card.innerHTML = `
      <div class="result-poster"></div>
      <div class="result-title"></div>
      <div class="result-sub"></div>`;
    const poster = card.querySelector(".result-poster");
    if (r.poster) {
      poster.style.backgroundImage = `url("${r.poster.replace(/"/g, "%22")}")`;
    } else {
      poster.classList.add("no-poster");
      poster.setAttribute("aria-hidden", "true");
    }
    card.querySelector(".result-title").textContent =
      r.year ? `${r.title} (${r.year})` : r.title;
    card.querySelector(".result-sub").textContent =
      r.kind === "series" ? "Series" : "Movie";
    card.addEventListener("click", () => openDetail(r));
    searchResults.appendChild(card);
  }
}

detailBack.addEventListener("click", () => {
  resultDetail.classList.add("hidden");
  searchResults.classList.remove("hidden");
});

async function openDetail(result) {
  currentResult = result;
  currentDetail = null;
  searchResults.classList.add("hidden");
  resultDetail.classList.remove("hidden");
  searchError.classList.add("hidden");
  seriesPicker.classList.add("hidden");

  detailPoster.src = result.poster || "";
  detailPoster.classList.toggle("hidden", !result.poster);
  detailTitle.textContent = result.title;
  detailSub.textContent = "Loading…";
  streamDownload.disabled = true;
  episodeList.innerHTML = `<li class="results-status">Loading episodes…</li>`;
  epCount.textContent = "";

  // Always load the detail: even a movie needs its real episode id to resolve.
  try {
    currentDetail = await invoke("get_series_detail", {
      site: result.site,
      id: result.id,
      translation: translation(),
    });
  } catch (err) {
    episodeList.innerHTML = "";
    detailSub.textContent = "";
    searchError.textContent = String(err);
    searchError.classList.remove("hidden");
    // A declared movie can still be attempted with a default episode id.
    streamDownload.disabled = result.kind !== "movie";
    return;
  }

  streamDownload.disabled = false;
  if (currentDetail.kind === "movie") {
    detailSub.textContent = "Movie";
    seriesPicker.classList.add("hidden");
    return;
  }
  detailSub.textContent = "Series";
  seriesPicker.classList.remove("hidden");
  renderSeasons();
}

function renderSeasons() {
  seasonSelect.innerHTML = "";
  for (const s of currentDetail.seasons) {
    const opt = document.createElement("option");
    opt.value = String(s.number);
    opt.textContent = `Season ${s.number} (${s.episodes.length})`;
    seasonSelect.appendChild(opt);
  }
  seasonSelect.classList.toggle("hidden", currentDetail.seasons.length <= 1);
  renderEpisodes();
}

seasonSelect.addEventListener("change", renderEpisodes);

function currentSeason() {
  const num = parseInt(seasonSelect.value, 10);
  return (
    currentDetail.seasons.find((s) => s.number === num) || currentDetail.seasons[0]
  );
}

function renderEpisodes() {
  const season = currentSeason();
  episodeList.innerHTML = "";
  season.episodes.forEach((ep, i) => {
    const row = document.createElement("label");
    row.className = "episode-row";
    row.innerHTML = `
      <input type="checkbox" class="ep-check" />
      <span class="episode-idx"></span>
      <span class="episode-title"></span>`;
    row.querySelector(".episode-idx").textContent = ep.number;
    row.querySelector(".episode-title").textContent = ep.title || `Episode ${ep.number}`;
    const check = row.querySelector(".ep-check");
    check.dataset.index = String(i);
    check.addEventListener("change", updateEpCount);
    episodeList.appendChild(row);
  });
  updateEpCount();
}

// Expand a range string ("5-12, 15") plus ticked boxes into the checked set.
function parseRange(str, maxIndex) {
  const set = new Set();
  for (const part of str.split(",")) {
    const t = part.trim();
    if (!t) continue;
    const m = t.match(/^(\d+)\s*-\s*(\d+)$/);
    if (m) {
      let a = parseInt(m[1], 10);
      let b = parseInt(m[2], 10);
      if (a > b) [a, b] = [b, a];
      for (let n = a; n <= b; n++) if (n >= 1 && n <= maxIndex) set.add(n);
    } else if (/^\d+$/.test(t)) {
      const n = parseInt(t, 10);
      if (n >= 1 && n <= maxIndex) set.add(n);
    }
  }
  return set;
}

// Apply the range field to the checkboxes (by 1-based episode position).
epRange.addEventListener("input", () => {
  const season = currentSeason();
  if (!season) return;
  const set = parseRange(epRange.value, season.episodes.length);
  if (!epRange.value.trim()) return;
  episodeList.querySelectorAll(".ep-check").forEach((c) => {
    c.checked = set.has(Number(c.dataset.index) + 1);
  });
  updateEpCount();
});

epSelectAll.addEventListener("click", () => {
  const boxes = [...episodeList.querySelectorAll(".ep-check")];
  const allChecked = boxes.every((c) => c.checked);
  boxes.forEach((c) => (c.checked = !allChecked));
  epSelectAll.textContent = allChecked ? "Select all" : "Clear all";
  updateEpCount();
});

function updateEpCount() {
  const n = episodeList.querySelectorAll(".ep-check:checked").length;
  epCount.textContent = n ? `${n} selected` : "";
}

function selectedEpisodes() {
  const season = currentSeason();
  const picked = [];
  episodeList.querySelectorAll(".ep-check:checked").forEach((c) => {
    const ep = season.episodes[Number(c.dataset.index)];
    if (ep) {
      picked.push({
        episode: ep.number,
        episodeId: ep.id,
        title: ep.title || `Episode ${ep.number}`,
      });
    }
  });
  return picked;
}

streamDownload.addEventListener("click", downloadSelection);

async function downloadSelection() {
  if (!currentResult) return;
  const isMovie =
    currentResult.kind === "movie" || seriesPicker.classList.contains("hidden");

  let episodes;
  let itemCount;
  if (isMovie) {
    // A movie downloads as a single-episode batch, using the one available
    // episode's number/id ("1" for AllAnime movies as a fallback).
    const ep = currentDetail?.seasons?.[0]?.episodes?.[0];
    episodes = [
      {
        episode: ep ? ep.number : "1",
        episodeId: ep ? ep.id : "1",
        title: currentResult.title,
      },
    ];
    itemCount = 1;
  } else {
    episodes = selectedEpisodes();
    if (!episodes.length) {
      searchError.textContent = "Select at least one episode.";
      searchError.classList.remove("hidden");
      return;
    }
    itemCount = episodes.length;
  }

  const params = {
    kind: "stream",
    site: currentResult.site,
    showId: currentResult.id,
    title: currentResult.title,
    episodes,
    translation: translation(),
    quality: streamQuality.value,
    destDir,
    playlistMode: itemCount > 1,
    itemCount,
  };
  const jobId = nextJobId++;
  await saveSetting("nextJobId", nextJobId);
  const job = createJobCard(jobId, params, {
    titles: episodes.map((e) => e.title),
  });
  await enqueueJob(job, { fresh: true });
  showView("download");
}

/* ---------- deep-link handler ---------- */

async function handleDeepLink({ action, url, quality }) {
  showView("download");
  urlInput.value = url;

  if (action === "analyze") {
    await analyze();
  } else if (action === "pluck") {
    if (quality) qualitySelect.value = quality;
    if (!currentMeta) {
      await analyze();
    }
    if (currentMeta) {
      pluckBtn.click();
    }
  }
}

listen("deep-link-received", async (event) => {
  await handleDeepLink(event.payload);
});

/* ---------- terms of use ---------- */

const termsOverlay = document.getElementById("terms-overlay");
const termsAcceptBtn = document.getElementById("terms-accept-btn");
const viewTermsBtn = document.getElementById("view-terms-btn");

function termsAccepted() {
  try {
    return localStorage.getItem(TERMS_KEY) === "true";
  } catch {
    return false;
  }
}

function acceptTerms() {
  try {
    localStorage.setItem(TERMS_KEY, "true");
  } catch { /* localStorage may be unavailable in some contexts */ }
  termsOverlay.classList.add("hidden");
}

function showTerms() {
  termsOverlay.classList.remove("hidden");
}

termsAcceptBtn.addEventListener("click", acceptTerms);

viewTermsBtn.addEventListener("click", showTerms);

/* ---------- external links ---------- */

const WEBSITE_URL = "https://www.xyruscode.com/software";
const DISCORD_URL = "https://discord.com/users/423390522296500224";

const websiteBtn = document.getElementById("website-btn");
const discordBtn = document.getElementById("discord-btn");

websiteBtn.addEventListener("click", () => {
  try {
    openUrl(WEBSITE_URL);
  } catch (e) {
    console.error("Failed to open website", e);
  }
});

discordBtn.addEventListener("click", () => {
  try {
    openUrl(DISCORD_URL);
  } catch (e) {
    console.error("Failed to open Discord", e);
  }
});

/* ---------- updater ---------- */

const autoUpdateCheck = document.getElementById("auto-update-check");
const checkUpdatesBtn = document.getElementById("check-updates-btn");

// Tauri v2's check() resolves to an Update object or null. The old v1 shape
// ({ shouldUpdate, manifest }) made every available update read as "up to
// date", so hold on to the returned object — install lives on it.
let updater = null;
let availableUpdate = null;
let updateCheckInFlight = false;
let lastUpdateCheckAt = 0;
const UPDATE_RECHECK_MS = 60_000;

function getUpdater() {
  if (!updater && window.__TAURI__?.updater) {
    updater = window.__TAURI__.updater;
  }
  return updater;
}

function setCheckButton(text, disabled = false) {
  if (!checkUpdatesBtn) return;
  checkUpdatesBtn.textContent = text;
  checkUpdatesBtn.disabled = disabled;
}

async function initUpdater() {
  // Load auto-update preference from settings
  try {
    autoUpdateCheck.checked = appSettings.autoUpdate;
  } catch {
    // default is checked (set in HTML)
  }
}

autoUpdateCheck.addEventListener("change", async () => {
  await saveSetting("autoUpdate", autoUpdateCheck.checked);
});

function showUpdateBanner(version, body) {
  // Remove any existing banner
  const existing = document.querySelector(".update-banner");
  if (existing) existing.remove();

  const banner = document.createElement("div");
  banner.className = "update-banner";
  banner.innerHTML = `
    <div class="update-banner-copy">
      <strong></strong>
      <p class="update-banner-notes"></p>
    </div>
    <div class="update-actions">
      <button id="update-install-btn" class="primary">Download &amp; Install</button>
      <button id="update-dismiss-btn" class="quiet">Dismiss</button>
    </div>
  `;

  // Release notes are external data; never splice them into markup.
  banner.querySelector("strong").textContent = `Video Plucker v${version} is available`;
  const notes = banner.querySelector(".update-banner-notes");
  if (body) {
    notes.textContent = body;
  } else {
    notes.remove();
  }
  document.body.appendChild(banner);

  banner.querySelector("#update-install-btn").addEventListener("click", async () => {
    banner.remove();
    await installUpdate();
  });

  banner.querySelector("#update-dismiss-btn").addEventListener("click", () => {
    banner.remove();
  });
}

async function installUpdate() {
  if (!availableUpdate) {
    // Only possible after a reload; re-check rather than failing silently.
    await checkForUpdates({ force: true });
  }
  const update = availableUpdate;
  if (!update) return;

  setCheckButton("Installing…", true);

  try {
    await update.downloadAndInstall();
    availableUpdate = null;
    setCheckButton("Restart to finish", false);
  } catch (err) {
    console.error("Update install failed:", err);
    setCheckButton("Install failed", false);
    setTimeout(() => setCheckButton("Check for updates", false), 3000);
  }
}

async function checkForUpdates({ silent = false, force = false } = {}) {
  const u = getUpdater();
  if (!u || updateCheckInFlight) return;

  const now = Date.now();
  if (!force && now - lastUpdateCheckAt < UPDATE_RECHECK_MS) return;
  lastUpdateCheckAt = now;
  updateCheckInFlight = true;
  setCheckButton("Checking…", true);

  try {
    const update = await u.check();

    if (update) {
      availableUpdate = update;
      showUpdateBanner(update.version || "new", update.body || "");
      setCheckButton(`v${update.version} available`, false);
    } else {
      availableUpdate = null;
      if (!silent) {
        setCheckButton("Up to date", false);
        setTimeout(() => setCheckButton("Check for updates", false), 2000);
      } else {
        setCheckButton("Check for updates", false);
      }
    }
  } catch (err) {
    console.error("Update check failed:", err);
    if (!silent) {
      setCheckButton("Check failed", false);
      setTimeout(() => setCheckButton("Check for updates", false), 3000);
    } else {
      setCheckButton("Check for updates", false);
    }
  } finally {
    updateCheckInFlight = false;
    if (checkUpdatesBtn?.textContent === "Checking…") {
      setCheckButton("Check for updates", false);
    }
  }
}

checkUpdatesBtn.addEventListener("click", () => checkForUpdates({ force: true }));

// A window that spent time in the background can miss the startup check.
// Recheck when it becomes visible again, throttled so focus flicker cannot
// hammer the release endpoint.
document.addEventListener("visibilitychange", () => {
  if (document.visibilityState === "visible" && autoUpdateCheck.checked) {
    checkForUpdates();
  }
});

/* ---------- init ---------- */

initSearchView();

if (!termsAccepted()) {
  showTerms();
}

initSettings()
  .then(async () => {
    await initUpdater();

    // Auto-check for updates on startup if enabled. A silent check still
    // surfaces the banner when an Update comes back.
    if (autoUpdateCheck.checked) {
      checkForUpdates({ silent: true });
    }

    // Initialize settings page
    await initSettingsPage();

    // Check for a deep-link that arrived before the frontend was ready.
    return invoke("consume_deep_link").then((payload) => {
      if (payload) handleDeepLink(payload);
    });
  })
  .catch((err) => {
    analyzeError.textContent = `Failed to load settings: ${err}`;
    analyzeError.classList.remove("hidden");
  });
/* ---------- settings page ---------- */

const downloadModeSelect = document.getElementById("download-mode-select");
const maxConcurrentSelect = document.getElementById("max-concurrent-select");
const concurrentDownloadsRow = document.getElementById("concurrent-downloads-row");
const settingsQualitySelect = document.getElementById("settings-quality-select");
const settingsDestDir = document.getElementById("settings-dest-dir");
const settingsBrowseBtn = document.getElementById("settings-browse-btn");
const appVersionSpan = document.getElementById("app-version");

async function initSettingsPage() {
  // Load current settings into UI
  downloadModeSelect.value = appSettings.downloadMode;
  maxConcurrentSelect.value = appSettings.maxConcurrentDownloads.toString();
  settingsQualitySelect.value = appSettings.quality;
  
  // Update concurrent downloads visibility
  updateConcurrentDownloadsVisibility();
  
  // Set destination directory
  settingsDestDir.textContent = destDir;
  settingsDestDir.title = destDir;

  // Get app version
  try {
    const version = await invoke("get_app_version");
    appVersionSpan.textContent = version;
    updateStatusBar();
  } catch {
    appVersionSpan.textContent = "Unknown";
    updateStatusBar();
  }

  // Event listeners
  downloadModeSelect.addEventListener("change", async () => {
    await saveSetting("downloadMode", downloadModeSelect.value);
    updateConcurrentDownloadsVisibility();
  });

  maxConcurrentSelect.addEventListener("change", async () => {
    await saveSetting("maxConcurrentDownloads", parseInt(maxConcurrentSelect.value));
  });

  // One quality, two controls: either select writes the same setting and
  // mirrors the value into the other, so they can never disagree.
  settingsQualitySelect.addEventListener("change", async () => {
    qualitySelect.value = settingsQualitySelect.value;
    await saveSetting("quality", settingsQualitySelect.value);
  });

  settingsBrowseBtn.addEventListener("click", async () => {
    const picked = await open({ directory: true, defaultPath: destDir });
    if (picked) {
      destDir = picked;
      settingsDestDir.textContent = destDir;
      settingsDestDir.title = destDir;
      renderDestDir(); // Update main UI
      await saveSetting("destDir", destDir);
    }
  });

  exportDumpBtn.addEventListener("click", exportDump);
  importDumpBtn.addEventListener("click", importDump);

  clearTempBtn.addEventListener("click", async () => {
    clearTempBtn.disabled = true;
    clearTempStatus.textContent = "Clearing…";
    try {
      const removed = await invoke("clear_temp", { destDir });
      clearTempStatus.textContent =
        removed === 0 ? "Temp folder is already clean." : `Removed ${removed} item(s) from .incoming.`;
    } catch (err) {
      clearTempStatus.textContent = String(err);
    } finally {
      clearTempBtn.disabled = false;
    }
  });
}

function updateConcurrentDownloadsVisibility() {
  const isParallel = downloadModeSelect.value === "parallel";
  concurrentDownloadsRow.classList.toggle("enabled", isParallel);
}
/* ---------- settings panes (DESIGN.md §6.4) ---------- */

document.querySelectorAll(".settings-nav-item").forEach((btn) => {
  btn.addEventListener("click", () => {
    document.querySelectorAll(".settings-nav-item").forEach((b) => b.classList.toggle("active", b === btn));
    document.querySelectorAll(".settings-pane").forEach((pane) => {
      pane.classList.toggle("active", pane.id === btn.dataset.pane);
    });
  });
});

/* ---------- window chrome (DESIGN.md §5) ----------
   The custom title bar exists only on Windows. Elsewhere the native window
   decorations stay on and this file must not draw a second title bar. */

function applyChrome() {
  const windows = /Windows/i.test(navigator.userAgent);
  document.body.dataset.chrome = windows ? "windows" : "native";
  windowControls.hidden = !windows;
  for (const el of [document.querySelector(".titlebar"), document.querySelector(".titlebar-brand")]) {
    el.toggleAttribute("data-tauri-drag-region", windows);
  }
  return windows;
}

function currentWindow() {
  return window.__TAURI__?.window?.getCurrentWindow?.() ?? null;
}

if (applyChrome()) {
  winMinimize.addEventListener("click", () => currentWindow()?.minimize());
  winMaximize.addEventListener("click", () => currentWindow()?.toggleMaximize());
  // Close hides to the tray, matching the Rust CloseRequested handler.
  winClose.addEventListener("click", () => currentWindow()?.hide());

  const syncMaximize = async () => {
    const w = currentWindow();
    const max = w ? await w.isMaximized() : false;
    winMaximize.setAttribute("aria-label", max ? "Restore" : "Maximise");
  };
  currentWindow()?.onResized?.(syncMaximize);
  syncMaximize();
}