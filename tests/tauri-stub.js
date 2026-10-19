// A stand-in for the Tauri bridge, injected before any page script runs.
//
// The frontend destructures window.__TAURI__ at module top level, so this has to
// exist before main.js evaluates or the whole app throws on load.
//
// The store stub is backed by a Map with the subset of tauri-plugin-store's API
// the app actually calls: load/get/set/delete/entries, plus autoSave.

function makeStore(seed) {
  const data = new Map(Object.entries(seed ?? {}));
  return {
    async get(key) {
      return data.has(key) ? data.get(key) : null;
    },
    async set(key, value) {
      data.set(key, value);
    },
    async delete(key) {
      data.delete(key);
    },
    async entries() {
      return [...data.entries()];
    },
  };
}

const settings = makeStore({
  destDir: "C:\\Users\\test\\Videos",
  quality: "1080",
  cookiesBrowser: "none",
  downloadMode: "queue",
  maxConcurrentDownloads: 3,
  autoUpdate: true,
  nextJobId: 1,
});

// Two parked items and two download records, so the smoke test has something
// to assert about the rows it renders rather than only ever seeing empty lists.
const parked = {
  "1": {
    jobId: 1,
    params: {
      url: "https://www.youtube.com/watch?v=dQw4w9WgXcQ",
      quality: "1080",
      title: "A Parked Video",
      destDir: "C:\\Users\\test\\Videos",
      playlistMode: false,
      itemCount: 1,
    },
    titles: [],
    completed: 0,
    failureKind: null,
    friendly: "Sign in to confirm your age",
    rawErrors: "",
    parkedAt: 1700000000000,
  },
};

const plucks = {
  "9": {
    jobId: 9,
    url: "https://www.youtube.com/watch?v=abcdefghijk",
    quality: "1080",
    destDir: "C:\\Users\\test\\Videos",
    playlistMode: false,
    title: "An Interrupted Download",
    itemCount: 1,
    completed: 0,
    status: "interrupted",
  },
};

// The Terms of Use modal is a full-screen overlay shown on first launch. The smoke test
// clicks tabs, so it has to be out of the way: accept the terms the way a returning
// user has. Without this the overlay swallows every click in the run.
//
// The app compares against the literal string "true", so "1" is not enough.
try {
  window.localStorage.setItem("video-plucker-terms-accepted", "true");
} catch {
  // Storage disabled; the app treats that as "not accepted" and shows the modal.
}

// Every command call is recorded so a test can assert on the argument shape. A Tauri
// command whose parameters the frontend does not supply fails at the boundary with
// "missing required key", which is easy to miss in review and trivial to pin down here.
const invokeLog = [];
window.__invokeLog = invokeLog;
const opened = {};

window.__TAURI__ = {
  core: {
    async invoke(cmd, args) {
      invokeLog.push({ cmd, args });
      switch (cmd) {
        case "list_cookies":
          return [
            { name: "x", path: "C:\\cookies\\x.txt" },
            { name: "vk_video", path: "C:\\cookies\\vk_video.txt" },
          ];
        case "list_sites":
          return [
            { id: "luciferdonghua", label: "Lucifer Donghua", available: true },
            { id: "allanime", label: "AllAnime", available: false },
          ];
        case "get_app_version":
          return "4.9.0";
        case "consume_deep_link":
          // No deep link pending in a test session.
          return null;
        case "search_content":
          return [];
        case "start_pluck":
          // Recorded rather than stubbed so a test can assert on what would have been
          // handed to yt-dlp, such as an empty -P from a restored queue item.
          return { jobId: 99 };
        case "export_dump":
          return {
            prefsKeys: 2,
            cookieFiles: 2,
            queueItems: 1,
            skippedQueueItems: 0,
            appVersion: "4.9.0",
          };
        case "import_dump":
          // Mirrors ImportResult in src-tauri/src/dump.rs, including the queue record's
          // full field set. A stub with a thinner shape lets a frontend bug pass here that
          // the real command would trip over.
          return {
            prefsKeys: 1,
            cookieFiles: 1,
            queueItems: 1,
            skippedQueueItems: 0,
            appVersion: "7.1.0",
            prefs: { autoUpdate: false },
            queue: [
              {
                jobId: "restored-1",
                params: {
                  url: "https://youtu.be/x",
                  quality: "1080",
                  title: "Restored",
                  // No destDir: the dump format has nowhere to put one, and the path the
                  // exporting machine had would be wrong here. The frontend must supply it.
                  destDir: "",
                  playlistMode: false,
                  itemCount: 1,
                },
                titles: [],
                completed: 0,
                failureKind: null,
                friendly: "Restored from backup",
                rawErrors: "",
                parkedAt: 1,
              },
            ],
          };
        default:
          throw new Error(`stub: unhandled command "${cmd}"`);
      }
    },
  },
  event: {
    // Returns an unlisten function like the real plugin does.
    async listen() {
      return () => {};
    },
    async emit() {},
  },
  dialog: {
    async open() {
      // A non-null path so file-picking flows actually run rather than bailing early.
      return "C:\\picked\\video-plucker-dump-7.1.0-1.json";
    },
    async save() {
      return "C:\\picked\\video-plucker-dump-4.8.0-1.json";
    },
    async message() {
      return true;
    },
  },
  store: {
    // One store instance per file. tauri-plugin-store hands back the same backing store
    // for a given path, so handing back a fresh Map each call would hide exactly the
    // persistence bugs these tests exist to catch.
    async load(name) {
      if (!opened[name]) {
        const seed =
          name === "settings.json" ? settings
          : name === "plucks.json" ? plucks
          : name === "queue.json" ? parked
          : {};
        opened[name] = makeStore(seed);
      }
      return opened[name];
    },
  },
  opener: {
    async revealItemInDir() {},
    async openUrl() {},
  },
  path: {
    async downloadDir() {
      return "C:\\Users\\test\\Videos";
    },
  },
  app: {
    async getVersion() {
      return "4.9.0";
    },
  },
  updater: {
    async check() {
      return { shouldUpdate: false };
    },
  },
  // The frameless window API. Recorded so a test can assert that the custom
  // controls call the right window verb; hide() is what keeps close-to-tray.
  window: {
    getCurrentWindow() {
      return {
        minimize: async () => invokeLog.push({ cmd: "window.minimize", args: {} }),
        toggleMaximize: async () => invokeLog.push({ cmd: "window.toggleMaximize", args: {} }),
        hide: async () => invokeLog.push({ cmd: "window.hide", args: {} }),
        isMaximized: async () => false,
        onResized: async () => () => {},
      };
    },
  },
};