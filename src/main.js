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
const downloadBtn = document.getElementById("download-btn");
const downloadsEl = document.getElementById("downloads");

let store = null;
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

/* ---------- settings ---------- */

async function initSettings() {
  store = await load("settings.json", { autoSave: true });
  destDir = (await store.get("destDir")) || (await downloadDir());
  const savedQuality = await store.get("quality");
  if (savedQuality) qualitySelect.value = savedQuality;
  renderDestDir();
}

function renderDestDir() {
  destDirEl.textContent = destDir;
  destDirEl.title = destDir;
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
  downloadBtn.disabled = true;
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
  downloadBtn.disabled = true;
  try {
    currentMeta = await invoke("fetch_metadata", {
      url,
      playlistMode: playlistMode(),
    });
    renderMeta(currentMeta);
    downloadBtn.disabled = false;
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
  downloadBtn.disabled = true;
  currentMeta = null;
});

function renderMeta(meta) {
  metaThumb.src = meta.thumbnail || "";
  metaThumb.classList.toggle("hidden", !meta.thumbnail);
  metaTitle.textContent = meta.title;

  if (meta.kind === "playlist") {
    metaSub.textContent = `Playlist · ${meta.entryCount} videos`;
    metaEntries.innerHTML = "";
    for (const title of meta.entries) {
      const li = document.createElement("li");
      li.textContent = title;
      metaEntries.appendChild(li);
    }
    if (meta.entryCount > meta.entries.length) {
      const li = document.createElement("li");
      li.textContent = `… and ${meta.entryCount - meta.entries.length} more`;
      metaEntries.appendChild(li);
    }
    metaEntries.classList.remove("hidden");
  } else {
    metaSub.textContent = fmtDuration(meta.duration);
    metaEntries.classList.add("hidden");
  }

  // grey out qualities the video doesn't offer (skip for playlists — unknown)
  const maxHeight = meta.heights?.length ? Math.max(...meta.heights) : null;
  for (const opt of qualitySelect.options) {
    const h = parseInt(opt.value, 10);
    opt.disabled =
      meta.kind === "video" && maxHeight != null && !isNaN(h) && h > maxHeight;
  }
  if (qualitySelect.selectedOptions[0]?.disabled) qualitySelect.value = "best";

  metaCard.classList.remove("hidden");
}

/* ---------- downloads ---------- */

function createJobCard(jobId, title, isPlaylist, itemCount) {
  const card = document.createElement("div");
  card.className = "job-card";
  card.innerHTML = `
    <div class="job-head">
      <span class="job-title"></span>
      <button class="job-cancel">Cancel</button>
      <button class="job-open hidden">Open folder</button>
    </div>
    <div class="job-item-line hidden"></div>
    <div class="bar overall"><div class="bar-fill"></div></div>
    <div class="bar item hidden"><div class="bar-fill"></div></div>
    <div class="job-stats">
      <span class="job-speed"></span>
      <span class="job-eta"></span>
      <span class="job-status">Starting…</span>
    </div>
    <div class="job-errors hidden"></div>
  `;
  card.querySelector(".job-title").textContent = title;
  downloadsEl.prepend(card);

  const job = {
    id: jobId,
    isPlaylist,
    itemCount: itemCount || 1,
    completed: 0,
    lastFile: null,
    card,
    itemLine: card.querySelector(".job-item-line"),
    overallFill: card.querySelector(".bar.overall .bar-fill"),
    itemBar: card.querySelector(".bar.item"),
    itemFill: card.querySelector(".bar.item .bar-fill"),
    speedEl: card.querySelector(".job-speed"),
    etaEl: card.querySelector(".job-eta"),
    statusEl: card.querySelector(".job-status"),
    errorsEl: card.querySelector(".job-errors"),
    cancelBtn: card.querySelector(".job-cancel"),
    openBtn: card.querySelector(".job-open"),
  };

  if (isPlaylist) {
    job.itemBar.classList.remove("hidden");
    job.itemLine.classList.remove("hidden");
    job.itemLine.textContent = `0 / ${job.itemCount}`;
  }

  job.cancelBtn.addEventListener("click", async () => {
    job.cancelBtn.disabled = true;
    try {
      await invoke("cancel_download", { jobId });
    } catch {
      job.cancelBtn.disabled = false;
    }
  });

  job.openBtn.addEventListener("click", () => {
    if (job.lastFile) revealItemInDir(job.lastFile);
  });

  jobs.set(jobId, job);
  return job;
}

downloadBtn.addEventListener("click", async () => {
  if (!currentMeta) return;
  const url = urlInput.value.trim();
  const isPlaylist = currentMeta.kind === "playlist";
  const jobId = nextJobId++;
  const job = createJobCard(
    jobId,
    currentMeta.title,
    isPlaylist,
    currentMeta.entryCount,
  );
  try {
    await invoke("start_download", {
      jobId,
      url,
      quality: qualitySelect.value,
      destDir,
      playlistMode: isPlaylist,
    });
    job.statusEl.textContent = "Downloading…";
  } catch (err) {
    finishJob(job, { ok: false, cancelled: false, error: String(err) });
  }
});

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
  } else if (cancelled) {
    job.statusEl.textContent = "Cancelled";
    job.statusEl.classList.add("cancelled");
  } else {
    job.statusEl.textContent = "Failed";
    job.statusEl.classList.add("fail");
    if (error) appendError(job, error);
    if (job.lastFile) job.openBtn.classList.remove("hidden");
  }
}

function appendError(job, message) {
  job.errorsEl.classList.remove("hidden");
  const line = document.createElement("div");
  line.textContent = message;
  job.errorsEl.appendChild(line);
}

/* ---------- backend events ---------- */

listen("download://item-start", ({ payload }) => {
  const job = jobs.get(payload.jobId);
  if (!job) return;
  if (job.isPlaylist) {
    if (payload.itemCount > 1) job.itemCount = payload.itemCount;
    job.itemLine.textContent = `${payload.itemIndex} / ${job.itemCount} — ${payload.title}`;
    job.itemFill.style.width = "0%";
  }
  job.statusEl.textContent = "Downloading…";
});

listen("download://progress", ({ payload }) => {
  const job = jobs.get(payload.jobId);
  if (!job) return;
  const fraction = payload.percent != null ? payload.percent / 100 : 0;
  if (job.isPlaylist) job.itemFill.style.width = `${(fraction * 100).toFixed(1)}%`;
  setOverall(job, fraction);
  job.speedEl.textContent = fmtSpeed(payload.speed);
  job.etaEl.textContent = payload.eta != null ? `ETA ${fmtEta(payload.eta)}` : "";
});

listen("download://item-done", ({ payload }) => {
  const job = jobs.get(payload.jobId);
  if (!job) return;
  job.completed = job.isPlaylist
    ? Math.min(job.completed + 1, job.itemCount)
    : 1;
  job.lastFile = payload.filepath;
  if (job.isPlaylist) {
    job.itemFill.style.width = "100%";
    job.itemLine.textContent = `${job.completed} / ${job.itemCount}`;
    setOverall(job, 0);
  } else {
    setOverall(job, 1);
    job.statusEl.textContent = "Processing…";
  }
});

listen("download://error", ({ payload }) => {
  const job = jobs.get(payload.jobId);
  if (job) appendError(job, payload.message);
});

listen("download://done", ({ payload }) => {
  const job = jobs.get(payload.jobId);
  if (job) finishJob(job, { ok: payload.ok, cancelled: payload.cancelled });
});

/* ---------- init ---------- */

initSettings().catch((err) => {
  analyzeError.textContent = `Failed to load settings: ${err}`;
  analyzeError.classList.remove("hidden");
});
