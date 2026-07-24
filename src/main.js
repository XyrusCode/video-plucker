const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;
const { open } = window.__TAURI__.dialog;
const { load } = window.__TAURI__.store;
const { revealItemInDir } = window.__TAURI__.opener;
const { downloadDir } = window.__TAURI__.path;

const urlInput = document.getElementById("url-input");
const playlistToggle = document.getElementById("playlist-toggle");
const playlistCheck = document.getElementById("playlist-check");
const analyzeBtn = document.getElementById("analyze-btn");
const analyzeError = document.getElementById("analyze-error");
const metaCard = document.getElementById("meta-card");
const metaThumb = document.getElementById("meta-thumb");
const metaTitle = document.getElementById("meta-title");
const metaSub = document.getElementById("meta-sub");
const metaEntries = document.getElementById("meta-entries");
const qualitySelect = document.getElementById("quality-select");
const cookiesSelect = document.getElementById("cookies-select");
const destDirEl = document.getElementById("dest-dir");
const browseBtn = document.getElementById("browse-btn");
const pluckBtn = document.getElementById("pluck-btn");
const plucksEl = document.getElementById("plucks");
const clearHistoryBtn = document.getElementById("clear-history-btn");

// Cookie manager elements
const cookieStatuses = {
  twitter: { status: document.getElementById("cookie-twitter-status"), clear: document.getElementById("cookie-twitter-clear"), btn: document.getElementById("cookie-twitter") },
  youtube: { status: document.getElementById("cookie-youtube-status"), clear: document.getElementById("cookie-youtube-clear"), btn: document.getElementById("cookie-youtube") },
  tiktok: { status: document.getElementById("cookie-tiktok-status"), clear: document.getElementById("cookie-tiktok-clear"), btn: document.getElementById("cookie-tiktok") },
};

// nav + search view
const navDownload = document.getElementById("nav-download");
const navSearch = document.getElementById("nav-search");
const viewDownload = document.getElementById("view-download");
const viewSearch = document.getElementById("view-search");
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

let store = null;
let plucksStore = null; // persisted pluck records, for resume after a crash
let destDir = "";
let currentMeta = null;
let nextJobId = 1;
const jobs = new Map(); // jobId -> job state + DOM refs

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

async function initSettings() {
  store = await load("settings.json", { autoSave: true });
  plucksStore = await load("plucks.json", { autoSave: true });
  destDir = (await store.get("destDir")) || (await downloadDir());
  const savedQuality = await store.get("quality");
  if (savedQuality) qualitySelect.value = savedQuality;
  const savedCookies = await store.get("cookiesBrowser");
  if (savedCookies) cookiesSelect.value = savedCookies;
  nextJobId = (await store.get("nextJobId")) || 1;
  renderDestDir();
  await restoreInterruptedPlucks();
  await refreshCookieStatus();
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
    await store.set("destDir", destDir);
  }
});

qualitySelect.addEventListener("change", async () => {
  await store.set("quality", qualitySelect.value);
});

cookiesSelect.addEventListener("change", async () => {
  await store.set("cookiesBrowser", cookiesSelect.value);
});

function cookiesFromBrowser() {
  const v = cookiesSelect.value;
  if (!v || v === "none") return null;
  return v;
}

/* ---------- cookie manager ---------- */

async function refreshCookieStatus() {
  let status;
  try {
    status = await invoke("get_platform_cookies_status");
  } catch {
    return;
  }
  for (const [platform, loaded] of Object.entries(status)) {
    const el = cookieStatuses[platform];
    if (!el) continue;
    if (loaded) {
      el.status.textContent = "Loaded";
      el.status.className = "cookie-status loaded";
      el.clear.classList.remove("hidden");
    } else {
      el.status.textContent = "Not loaded";
      el.status.className = "cookie-status not-loaded";
      el.clear.classList.add("hidden");
    }
  }
}

async function importCookies(platform) {
  const selected = await open({
    multiple: false,
    filters: [{ name: "Cookies", extensions: ["txt"] }],
  });
  if (!selected) return;
  try {
    await invoke("import_platform_cookies", { platform, sourcePath: selected });
    await refreshCookieStatus();
  } catch (err) {
    analyzeError.textContent = `Failed to import ${platform} cookies: ${err}`;
    analyzeError.classList.remove("hidden");
  }
}

async function clearCookies(platform) {
  try {
    await invoke("clear_platform_cookies", { platform });
    await refreshCookieStatus();
  } catch (err) {
    analyzeError.textContent = `Failed to clear ${platform} cookies: ${err}`;
    analyzeError.classList.remove("hidden");
  }
}

// Wire up cookie import/clear buttons
for (const [platform, el] of Object.entries(cookieStatuses)) {
  el.btn.addEventListener("click", () => importCookies(platform));
  el.clear.addEventListener("click", () => clearCookies(platform));
}

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

urlInput.addEventListener("input", () => {
  playlistToggle.classList.toggle("hidden", !isPlaylistUrl(urlInput.value));
  pluckBtn.disabled = true;
  currentMeta = null;
});

function playlistMode() {
  return isPlaylistUrl(urlInput.value.trim()) && playlistCheck.checked;
}

async function analyze() {
  const raw = urlInput.value.trim();
  const url = cleanPlatformUrl(raw);
  if (url !== raw) urlInput.value = url;
  if (!url) return;

  analyzeError.classList.add("hidden");
  analyzeBtn.disabled = true;
  analyzeBtn.textContent = "Analyzing…";
  pluckBtn.disabled = true;
  try {
    currentMeta = await invoke("fetch_metadata", {
      url,
      playlistMode: playlistMode(),
      cookiesFromBrowser: cookiesFromBrowser(),
    });
    renderMeta(currentMeta);
    pluckBtn.disabled = false;
  } catch (err) {
    metaCard.classList.add("hidden");
    let msg = String(err);
    // YouTube's bot check is only escapable with browser cookies — point the
    // user at the control that does it.
    if (/not a bot|Sign in to confirm/i.test(msg) && !cookiesFromBrowser()) {
      msg += "\n\nTip: set “YouTube login” to your browser to use its cookies.";
    }
    analyzeError.textContent = msg;
    analyzeError.classList.remove("hidden");
  } finally {
    analyzeBtn.disabled = false;
    analyzeBtn.textContent = "Analyze";
  }
}

analyzeBtn.addEventListener("click", analyze);
urlInput.addEventListener("keydown", (e) => {
  if (e.key === "Enter") analyze();
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

/* ---------- plucks ---------- */

// A "params" object fully describes a pluck: { url, quality, destDir,
// playlistMode, title, itemCount }. `titles` is an optional per-item title list
// (only available right after analyze, not after a restart).
function createJobCard(jobId, params, { completed = 0, titles = [] } = {}) {
  const isPlaylist = params.playlistMode;
  const itemCount = params.itemCount || 1;

  const card = document.createElement("div");
  card.className = "job-card";
  card.innerHTML = `
    <div class="job-head">
      <button class="job-expand hidden" title="Show items">▸</button>
      <span class="job-title"></span>
      <button class="job-cancel">Cancel</button>
      <button class="job-resume hidden">Resume</button>
      <button class="job-open hidden">Open folder</button>
      <button class="job-dismiss hidden" title="Remove">✕</button>
    </div>
    <div class="job-item-line hidden"></div>
    <div class="bar overall"><div class="bar-fill"></div></div>
    <div class="bar item hidden"><div class="bar-fill"></div></div>
    <div class="job-stats">
      <span class="job-speed"></span>
      <span class="job-eta"></span>
      <span class="job-status">Starting…</span>
    </div>
    <ul class="job-items hidden"></ul>
    <div class="job-errors hidden"></div>
  `;
  card.querySelector(".job-title").textContent = params.title;
  plucksEl.prepend(card);

  const job = {
    id: jobId,
    params,
    isPlaylist,
    itemCount,
    completed,
    activeIndex: 0,
    lastFile: null,
    titles,
    card,
    itemLine: card.querySelector(".job-item-line"),
    overallFill: card.querySelector(".bar.overall .bar-fill"),
    itemBar: card.querySelector(".bar.item"),
    itemFill: card.querySelector(".bar.item .bar-fill"),
    speedEl: card.querySelector(".job-speed"),
    etaEl: card.querySelector(".job-eta"),
    statusEl: card.querySelector(".job-status"),
    errorsEl: card.querySelector(".job-errors"),
    itemsEl: card.querySelector(".job-items"),
    cancelBtn: card.querySelector(".job-cancel"),
    resumeBtn: card.querySelector(".job-resume"),
    openBtn: card.querySelector(".job-open"),
    dismissBtn: card.querySelector(".job-dismiss"),
    expandBtn: card.querySelector(".job-expand"),
    itemRows: new Map(),
  };

  if (isPlaylist) {
    job.itemBar.classList.remove("hidden");
    job.itemLine.classList.remove("hidden");
    job.itemLine.textContent = `${completed} / ${itemCount}`;
    job.expandBtn.classList.remove("hidden");
    buildItemRows(job);
    job.expandBtn.addEventListener("click", () => {
      const open = job.itemsEl.classList.toggle("hidden") === false;
      job.expandBtn.textContent = open ? "▾" : "▸";
    });
  }

  job.cancelBtn.addEventListener("click", async () => {
    job.cancelBtn.disabled = true;
    try {
      await invoke("cancel_pluck", { jobId });
    } catch {
      job.cancelBtn.disabled = false;
    }
  });

  job.resumeBtn.addEventListener("click", () => resumeJob(job));
  job.dismissBtn.addEventListener("click", async () => {
    await removeRecord(jobId);
    jobs.delete(jobId);
    job.card.remove();
  });
  job.openBtn.addEventListener("click", () => {
    if (job.lastFile) revealItemInDir(job.lastFile);
  });

  jobs.set(jobId, job);
  return job;
}

function buildItemRows(job) {
  job.itemsEl.innerHTML = "";
  job.itemRows.clear();
  for (let i = 1; i <= job.itemCount; i++) {
    const li = document.createElement("li");
    li.className = "pl-item";
    li.dataset.state = "queued";
    li.innerHTML = `
      <span class="pl-idx"></span>
      <span class="pl-title"></span>
      <span class="pl-state">queued</span>
      <div class="pl-bar"><div class="pl-fill"></div></div>`;
    li.querySelector(".pl-idx").textContent = i;
    li.querySelector(".pl-title").textContent = job.titles[i - 1] || `Item ${i}`;
    job.itemsEl.appendChild(li);
    job.itemRows.set(i, {
      li,
      title: li.querySelector(".pl-title"),
      state: li.querySelector(".pl-state"),
      fill: li.querySelector(".pl-fill"),
    });
  }
  // when resuming, items already recorded in the archive are done
  for (let i = 1; i <= job.completed; i++) markItem(job, i, "done");
}

function markItem(job, index, state, title) {
  const row = job.itemRows.get(index);
  if (!row) return;
  if (title) row.title.textContent = title;
  row.li.dataset.state = state;
  if (state === "done") {
    row.state.textContent = "done";
    row.fill.style.width = "100%";
  } else if (state === "active") {
    row.state.textContent = "plucking";
    row.li.scrollIntoView({ block: "nearest" });
  } else if (state === "failed") {
    row.state.textContent = "skipped";
  }
}

// Start (or resume) a pluck. On resume we reuse the same jobId so its yt-dlp
// download archive lines up and finished items are skipped.
async function beginPluck(job, { fresh }) {
  job.card.classList.remove("done");
  job.statusEl.className = "job-status";
  job.statusEl.textContent = "Starting…";
  job.cancelBtn.classList.remove("hidden");
  job.cancelBtn.disabled = false;
  job.resumeBtn.classList.add("hidden");
  job.openBtn.classList.add("hidden");
  job.dismissBtn.classList.add("hidden");
  job.errorsEl.classList.add("hidden");
  job.errorsEl.innerHTML = "";

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
      });
    }
    job.statusEl.textContent = "Plucking…";
  } catch (err) {
    finishJob(job, { ok: false, cancelled: false, error: String(err) });
  }
}

function resumeJob(job) {
  job.completed = 0; // the archive is the source of truth; rebuild from events
  if (job.isPlaylist) buildItemRows(job);
  beginPluck(job, { fresh: false });
}

pluckBtn.addEventListener("click", async () => {
  if (!currentMeta) return;
  const isPlaylist = currentMeta.kind === "playlist";
  const params = {
    url: cleanPlatformUrl(urlInput.value.trim()),
    quality: qualitySelect.value,
    destDir,
    playlistMode: isPlaylist,
    title: currentMeta.title,
    itemCount: isPlaylist ? currentMeta.entryCount : 1,
    cookiesFromBrowser: cookiesFromBrowser(),
  };
  const jobId = nextJobId++;
  await store.set("nextJobId", nextJobId);
  const job = createJobCard(jobId, params, {
    titles: isPlaylist ? currentMeta.entries : [],
  });
  await beginPluck(job, { fresh: true });
});

// Re-create cards for plucks that were still active when the app last closed.
async function restoreInterruptedPlucks() {
  let entries;
  try {
    entries = await plucksStore.entries();
  } catch {
    return;
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
      await store.set("nextJobId", nextJobId);
    }
    job.cancelBtn.classList.add("hidden");
    job.resumeBtn.classList.remove("hidden");
    job.dismissBtn.classList.remove("hidden");
    job.statusEl.textContent = "Interrupted — resume to continue";
    job.statusEl.classList.add("cancelled");
  }
}

function setOverall(job, itemFraction) {
  const overall = job.isPlaylist
    ? (job.completed + itemFraction) / job.itemCount
    : itemFraction;
  job.overallFill.style.width = `${Math.min(100, overall * 100).toFixed(1)}%`;
}

function finishJob(job, { ok, cancelled, error }) {
  job.cancelBtn.classList.add("hidden");
  job.speedEl.textContent = "";
  job.etaEl.textContent = "";
  if (ok) {
    job.card.classList.add("done");
    job.overallFill.style.width = "100%";
    job.itemBar.classList.add("hidden");
    job.statusEl.textContent = "Completed";
    job.statusEl.classList.add("ok");
    if (job.lastFile) job.openBtn.classList.remove("hidden");
    removeRecord(job.id);
  } else if (cancelled) {
    job.statusEl.textContent = "Cancelled";
    job.statusEl.classList.add("cancelled");
    removeRecord(job.id);
  } else {
    job.statusEl.textContent = "Failed — resume to retry";
    job.statusEl.classList.add("fail");
    if (error) appendError(job, error);
    if (job.lastFile) job.openBtn.classList.remove("hidden");
    job.resumeBtn.classList.remove("hidden");
    job.dismissBtn.classList.remove("hidden");
    patchRecord(job.id, { status: "failed", completed: job.completed });
  }
}

function appendError(job, message) {
  job.errorsEl.classList.remove("hidden");
  const line = document.createElement("div");
  line.textContent = message;
  job.errorsEl.appendChild(line);
}

clearHistoryBtn.addEventListener("click", async () => {
  const toRemove = [];
  for (const [id, job] of jobs) {
    const status = job.card.querySelector(".job-status");
    const isFinished = status?.classList.contains("ok") || status?.classList.contains("cancelled") || status?.classList.contains("fail");
    if (isFinished) {
      toRemove.push(id);
    }
  }
  for (const id of toRemove) {
    const job = jobs.get(id);
    if (job) {
      await removeRecord(id);
      job.card.remove();
      jobs.delete(id);
    }
  }
});

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
    job.itemLine.textContent = `${payload.itemIndex} / ${job.itemCount} — ${payload.title}`;
    job.itemFill.style.width = "0%";
  }
  job.statusEl.textContent = "Plucking…";
});

listen("pluck://progress", ({ payload }) => {
  const job = jobs.get(payload.jobId);
  if (!job) return;
  const fraction = payload.percent != null ? payload.percent / 100 : 0;
  if (job.isPlaylist) {
    job.itemFill.style.width = `${(fraction * 100).toFixed(1)}%`;
    const idx = payload.itemIndex || job.activeIndex;
    const row = job.itemRows.get(idx);
    if (row) row.fill.style.width = `${(fraction * 100).toFixed(1)}%`;
  }
  setOverall(job, fraction);
  job.speedEl.textContent = fmtSpeed(payload.speed);
  job.etaEl.textContent = payload.eta != null ? `ETA ${fmtEta(payload.eta)}` : "";
});

listen("pluck://item-done", ({ payload }) => {
  const job = jobs.get(payload.jobId);
  if (!job) return;
  job.lastFile = payload.filepath;
  if (job.isPlaylist) {
    markItem(job, payload.itemIndex, "done");
    job.completed = Math.min(payload.itemIndex, job.itemCount);
    job.itemLine.textContent = `${job.completed} / ${job.itemCount}`;
    setOverall(job, 0);
    patchRecord(job.id, { completed: job.completed });
  } else {
    job.completed = 1;
    setOverall(job, 1);
    job.statusEl.textContent = "Processing…";
  }
});

listen("pluck://error", ({ payload }) => {
  const job = jobs.get(payload.jobId);
  if (!job) return;
  if (job.isPlaylist && job.activeIndex) markItem(job, job.activeIndex, "failed");
  appendError(job, payload.message);
});

listen("pluck://done", ({ payload }) => {
  const job = jobs.get(payload.jobId);
  if (job) finishJob(job, { ok: payload.ok, cancelled: payload.cancelled });
});

/* ---------- search view ---------- */

function showView(which) {
  const search = which === "search";
  viewSearch.classList.toggle("hidden", !search);
  viewDownload.classList.toggle("hidden", search);
  navSearch.classList.toggle("active", search);
  navDownload.classList.toggle("active", !search);
}

navDownload.addEventListener("click", () => showView("download"));
navSearch.addEventListener("click", () => showView("search"));

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
      <div class="result-meta">
        <span class="result-title"></span>
        <span class="badge badge-${r.kind}">${r.kind}</span>
      </div>`;
    const poster = card.querySelector(".result-poster");
    if (r.poster) {
      poster.style.backgroundImage = `url("${r.poster.replace(/"/g, "%22")}")`;
    } else {
      poster.classList.add("no-poster");
      poster.textContent = "▶";
    }
    card.querySelector(".result-title").textContent =
      r.year ? `${r.title} (${r.year})` : r.title;
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
    const li = document.createElement("li");
    li.className = "pl-item ep-row";
    li.innerHTML = `
      <input type="checkbox" class="ep-check" />
      <span class="pl-idx"></span>
      <span class="pl-title"></span>`;
    li.querySelector(".pl-idx").textContent = ep.number;
    li.querySelector(".pl-title").textContent = ep.title || `Episode ${ep.number}`;
    const check = li.querySelector(".ep-check");
    check.dataset.index = String(i);
    check.addEventListener("change", updateEpCount);
    episodeList.appendChild(li);
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
  await store.set("nextJobId", nextJobId);
  const job = createJobCard(jobId, params, {
    titles: episodes.map((e) => e.title),
  });
  await beginPluck(job, { fresh: true });
  showView("download");
}

/* ---------- init ---------- */

initSearchView();

initSettings().catch((err) => {
  analyzeError.textContent = `Failed to load settings: ${err}`;
  analyzeError.classList.remove("hidden");
});
