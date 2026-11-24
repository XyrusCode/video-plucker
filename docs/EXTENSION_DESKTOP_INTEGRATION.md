# Extension ↔ Desktop Integration Spec

This doc is the source of truth for the protocol between the **browser extension**
(`Xyrus-YT-Plucker-Extension`) and the **Rune desktop app** (`video-plucker`).
Both repos have a copy — keep them in sync.

---

## Architecture

```
Browser Extension (Chrome/Edge)          Desktop App (Rune)
┌──────────────────────────────┐        ┌──────────────────────────┐
│  right-click → "Pluck This"  │        │                          │
│  popup → "Open in Desktop"   │        │  yt-plucker:// URL       │
│              │               │        │       │                  │
│              ▼               │        │       ▼                  │
│  background.js builds URL    │──OS──▶│  single-instance argv    │
│  yt-plucker://analyze?...    │        │       │                  │
│                              │        │       ▼                  │
│                              │        │  handle_deep_link_argv() │
│                              │        │       │                  │
│                              │        │       ├─ emit event      │
│                              │        │       └─ PendingDeepLink │
│                              │        │              │           │
│                              │        │              ▼           │
│                              │        │  frontend main.js        │
│                              │        │  handleDeepLink(payload) │
└──────────────────────────────┘        └──────────────────────────┘
```

## Protocol URL Format

```
yt-plucker://{action}?url={encoded_url}&quality={encoded_quality}
```

| Param    | Required | Description                                     |
|----------|----------|-------------------------------------------------|
| `action` | yes      | `analyze` (fetch metadata) or `pluck` (download)|
| `url`    | yes      | URL-encoded video/page URL                      |
| `quality`| no       | Format code (e.g. `bestvideo+bestaudio`)         |

### Examples

```
yt-plucker://analyze?url=https%3A%2F%2Fwww.youtube.com%2Fwatch%3Fv%3DUMnizvKZpSc
yt-plucker://pluck?url=https%3A%2F%2Fwww.youtube.com%2Fwatch%3Fv%3DUMnizvKZpSc&quality=22
```

## Extension → Desktop Flow

### 1. Context Menu ("Pluck This Video")

`background.js` detects a right-click on a supported video page, builds the
protocol URL, and opens it:

```js
// Chrome blocks chrome.tabs.create with custom schemes, so we use a
// data: URL redirect — the page-initiated navigation is allowed.
chrome.tabs.create({
  url: `data:text/html;charset=utf-8,<script>location.href='${protocolUrl}'</script>`
});
```

### 2. Popup ("Open in Desktop App")

The extension popup sends a `launchDesktopApp` message to the background
service worker, which launches the protocol URL the same way.

## Desktop Deep-Link Handling

### Startup (single-instance + on_open_url)

```rust
rune::single_instance("tech.xyruscode.videoplucker"); // first line of main
App::new(spec).on_open_url(|s, url: &str, _| ui::handle_deep_link(s, url));
```

- `rune::single_instance` keeps one process and forwards a second launch's
  arguments over a per-user named pipe. `on_open_url` receives scheme links on
  Windows and macOS, including the one that launched the app.
- On Linux, Rune does not deliver `on_open_url`, so `main()` parses
  `std::env::args()` and calls `AppHandle::update` with the link.
- Both paths call the same `ui::handle_deep_link()`, which parses
  `yt-plucker://{action}?url=…&quality=…` with `url::Url`, switches to the Pluck
  view, fills the URL field and runs Analyze (and Pluck for the `pluck` action).

There is no separate event/listener race: Rune state lives in the same process
as the handler, so the link is applied directly to it. The `deep-link-received`
compat event is retained only for the shim, and is handled by the same function.

## Protocol Registration

Registration is now part of each installer, not a framework plugin:

- **Windows:** the NSIS installer writes `HKCU\Software\Classes\yt-plucker`
  with an empty `URL Protocol` value and
  `shell\open\command` = `"<install dir>\yt-plucker.exe" "%1"`.
- **macOS:** `Contents/Info.plist` declares `CFBundleURLTypes` with the
  `yt-plucker` scheme.
- **Linux:** the `.deb` ships
  `/usr/share/applications/yt-plucker.desktop` with
  `MimeType=x-scheme-handler/yt-plucker;`.

## Supported Sites

The extension detects these platforms and builds appropriate `yt-plucker://` URLs:

| Platform  | URL Pattern                   |
|-----------|-------------------------------|
| YouTube   | `youtube.com/watch?v=...`     |
| X/Twitter | `x.com/*/status/*`            |
| TikTok    | `tiktok.com/@*/*`             |

## Version Compatibility

| Extension | Desktop | Protocol |
|-----------|---------|----------|
| 1.1.0     | 4.3.0   | `yt-plucker://analyze?...` / `yt-plucker://pluck?...` |
| 1.2.5     | 4.4.0   | Same protocol; cookies.txt export feeds the desktop Cookie Manager |
| 1.2.5     | 4.5.1   | Same protocol; failure UX overhaul (Queue-for-later, issue reporting, self-healing yt-dlp) |
| 1.2.5     | 4.14.0  | Same protocol; the desktop adds a Saved tab, a batch link field and a retry |
| 1.2.5     | 4.15.0  | Same protocol; download ledger, duplicate check on analyze, dump v3 |
| 1.2.5     | 4.16.0  | Same protocol; unlimited parallel downloads, default-quality setting, tray teardown |
| 1.2.5     | 4.17.0  | Same protocol; history tab, per-source subfolders, download-all, clear temp files |
| 1.2.5     | 4.18.0  | Same protocol; immediate update detection, categorized Linux entry, Zorin-inspired look |
| 1.2.5     | 5.0.0   | Same protocol; new UI shell and engine updates |
| 1.3.0     | 6.0.2   | Same protocol; native Rune UI, installer-based scheme registration |

## Key Files

| Repo      | File                              | Role                              |
|-----------|-----------------------------------|-----------------------------------|
| Extension | `extension/background.js`         | Build & launch protocol URLs      |
| Extension | `extension/popup/popup.js`        | "Open in Desktop App" button      |
| Desktop   | `src-tauri/src/lib.rs`            | single_instance, argv, tray       |
| Desktop   | `src-tauri/src/ui.rs`             | `handle_deep_link()`              |
| Desktop   | `scripts/installer.nsi` / `*.desktop` / `Info.plist` | scheme registration |
| Both      | `docs/EXTENSION_DESKTOP_INTEGRATION.md` | This doc                   |

---

*Last updated: v6.0.2 (Rune UI, installer-based scheme registration; the protocol is unchanged)*
