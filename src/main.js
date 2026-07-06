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
const destDirEl = document.getElementById("dest-dir");
const browseBtn = document.getElementById("browse-btn");
const pluckBtn = document.getElementById("pluck-btn");
const plucksEl = document.getElementById("plucks");

let store = null;
let plucksStore = null; // persisted pluck records, for resume after a crash
let destDir = "";
let currentMeta = null;
let nextJobId = 1;
const jobs = new Map(); // jobId -> job state + DOM refs

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
  nextJobId = (await store.get("nextJobId")) || 1;
  renderDestDir();
  await restoreInterruptedPlucks();
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
  const url = urlInput.value.trim();
  if (!url) return;

  analyzeError.classList.add("hidden");
  analyzeBtn.disabled = true;
  analyzeBtn.textContent = "Analyzing…";
  pluckBtn.disabled = true;
  try {
    currentMeta = await invoke("fetch_metadata", {
      url,
      playlistMode: playlistMode(),
    });
    renderMeta(currentMeta);
    pluckBtn.disabled = false;
  } catch (err) {
    metaCard.classList.add("hidden");
    analyzeError.textContent = String(err);
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
    await invoke("start_pluck", {
      jobId: job.id,
      url: job.params.url,
      quality: job.params.quality,
      destDir: job.params.destDir,
      playlistMode: job.params.playlistMode,
    });
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
    url: urlInput.value.trim(),
    quality: qualitySelect.value,
    destDir,
    playlistMode: isPlaylist,
    title: currentMeta.title,
    itemCount: isPlaylist ? currentMeta.entryCount : 1,
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
    if (!rec || !rec.url) continue;
    const params = {
      url: rec.url,
      quality: rec.quality,
      destDir: rec.destDir,
      playlistMode: rec.playlistMode,
      title: rec.title,
      itemCount: rec.itemCount,
    };
    const job = createJobCard(rec.jobId, params, { completed: rec.completed || 0 });
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

/* ---------- init ---------- */

initSettings().catch((err) => {
  analyzeError.textContent = `Failed to load settings: ${err}`;
  analyzeError.classList.remove("hidden");
});
