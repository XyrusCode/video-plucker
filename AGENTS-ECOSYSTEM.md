# Video Plucker — Ecosystem Instructions

Three apps, one engine. All powered by **yt-dlp + ffmpeg** for downloading from
YouTube, X/Twitter, TikTok, and (desktop-only) streaming sites.

| App | Stack | Location |
|-----|-------|----------|
| **Android** | Kotlin + Jetpack Compose, Clean Architecture | `Video-Plucker-Android/` |
| **Desktop** | Tauri v2 (Rust + vanilla HTML/CSS/JS) | `Video-Plucker-Desktop/` |
| **Extension** | Chrome MV3 (vanilla JS), desktop-pairing only | `Video-Plucker-Extension/` |
| **Tsubarashi** | Expo SDK 55 + React Native, glustack v5 + nativewind v5 | `Video-Plucker/tsubarashi/` (temp member) |

App-specific instructions live in `Video-Plucker-Desktop/AGENTS.md` and
`Video-Plucker-Android/CLAUDE.md`. This file covers ecosystem-wide rules and the
integration points between the three apps.

---

## Shared Conventions (ecosystem-wide)

### No local builds — ever

**None of the three apps can be built locally.** The dev environment has no
JDK/Android SDK, no Rust MSVC toolchain, no Python properly configured. Always
push to GitHub and let CI build.

### Rinse-repeat CI strategy

1. Push your changes.
2. Poll CI logs: `gh run view <run-id> --log` (or `gh run list --branch <branch>`).
3. Fix any failures, push again.
4. Repeat until green.

### Shared engine: yt-dlp

yt-dlp is the download engine across all three apps. When YouTube, X, or TikTok
break yt-dlp extractors, all three apps are affected. Each app bundles its own
copy of yt-dlp — update the bundled copy in each app independently.

### Supported sites

| Site | Android | Desktop | Extension |
|------|---------|---------|-----------|
| YouTube | yes | yes | yes |
| X / Twitter | yes | yes | yes |
| TikTok | yes | yes | yes |
| Instagram | yes | no | no |
| Facebook | yes | no | no |
| Reddit | yes | no | no |
| VK | yes | no | no |
| Streaming sites (AllAnime, LuciferDonghua) | no | yes | no |
| Custom (any yt-dlp supported site via cookie) | yes | no | no |

### Independent versioning

Each app has its own independent version. There is no cross-app version coupling.
Bump the version in the app-specific location only.

### Tsubarashi build versioning (every build bumps patch)

Each Tsubarashi build that goes out MUST bump the **patch** version by exactly one
unless the user explicitly states it's a **major** or **minor** release. This lets
Android detect it as an update instead of reinstalling the same version.

- Default: `X.Y.Z` → `X.Y.(Z+1)` (e.g. `0.1.0` → `0.1.1`).
- Minor: `X.Y.Z` → `X.(Y+1).0` — only when the user says "minor release".
- Major: `X.Y.Z` → `(X+1).0.0` — only when the user says "major release".

Version lives in TWO places for Tsubarashi, bump both in sync:
`Tsubarashi/app.json` (`expo.version`) and `Tsubarashi/package.json`
(`version`). EAS `appVersionSource: "remote"` already auto-increments the
Android `versionCode` per build, so Android will treat each build as an update
as long as the version string also bumps. Never re-ship a build with the same
version that was already installed.

---

## Android App (`Video-Plucker-Android/`)

**Stack:** Kotlin + Jetpack Compose, Clean Architecture (ViewModel +
Coroutines/Flow), foreground `DownloadService` with WakeLock.

**Download engine:** [youtubedl-android](https://github.com/yausername/youtubedl-android)
v0.18.1 (JunkFood02 fork on JitPack), bundling Python + yt-dlp + ffmpeg per
ABI. yt-dlp self-updates on first launch via `updateYoutubeDL()`.

**Package:** `xyrus.code.ytplucker` · **Root project:** `Video-Plucker`

**Key files:**
| File | Purpose |
|------|---------|
| `app/build.gradle.kts` | Version (`versionCode` + `versionName`), signing, ABI splits, dependencies |
| `settings.gradle.kts` | Root project name, repositories (JitPack for youtubedl-android) |
| `gradle/libs.versions.toml` | Version catalog |
| `keystore/ytplucker.jks` | Shared signing key (alias/pw: `ytplucker`, identity: `xyrus.code.yt-plucker`) |
| `domain/model/Platforms.kt` | Single source of truth for supported platforms |
| `remoteconfig.template.json` | Firebase Remote Config template |

**Build:** `./gradlew assembleDebug` → APKs in `app/build/outputs/apk/debug/`.

**ABI splits:** `arm64-v8a`, `armeabi-v7a`, `x86_64` + universal APK.

**CI:** `.github/workflows/android-build.yml`
- Push/PR → unsigned debug APK (workflow artifact).
- Push `v*` tag → per-ABI APKs attached to a GitHub Release.

**Release mirroring:** `.github/workflows/release.yml`
- Push `v*` tag → builds per-ABI APKs, publishes GitHub Release, uploads to
  Cloudflare R2 bucket `video-plucker-releases` under `mobile/v<VERSION>/`.
- Mirror URLs: `https://releases.xyruscode.com/mobile/v<VERSION>/app-<abi>-release.apk`
- R2 auth: `${{ secrets.CLOUDFLARE_API_TOKEN }}` (via wrangler CLI).

**TikTok specifics:**
- Download args must include `--extractor-args tiktok:web_fallback=true` to
  avoid impersonator blocks.
- Photo/slideshow URLs (`/photo/` path segment) are unsupported — the app
  detects and surfaces this cleanly.

**Features:** In-app browser with floating download button, paste/analyze/download
tab, history tab (reads from device media library), Android share target, Android
TV (leanback launcher), Firebase Remote Config, Sentry crash reporting.

**See also:** `Video-Plucker-Android/CLAUDE.md` for app-specific AI coding rules.

---

## Desktop App (`Video-Plucker-Desktop/`)

**Stack:** Tauri v2 — Rust backend (`src-tauri/`) + vanilla HTML/CSS/JS frontend
(`src/`). Identifier: `tech.xyruscode.ytplucker`.

**Download engine:** yt-dlp + ffmpeg as sidecar binaries under
`src-tauri/binaries/` (not committed; downloaded in CI).

**Key files:**
| File | Purpose |
|------|---------|
| `src-tauri/tauri.conf.json` | App version, deep-link plugin config, bundle targets |
| `src-tauri/Cargo.toml` | Rust package version, dependencies |
| `src/main.js` | Frontend entry point, deep-link handler |
| `src-tauri/src/lib.rs` | Backend logic, deep-link parsing, `PendingDeepLink` state |
| `docs/EXTENSION_DESKTOP_INTEGRATION.md` | Extension ↔ Desktop protocol spec |

**Version lives in TWO places:** `src-tauri/tauri.conf.json` AND
`src-tauri/Cargo.toml`. Bump both per release, keep them in sync.

**CI:**
- `build-check.yml` — `cargo check` on `windows-latest` for every PR.
- `release.yml` — bump version in `src-tauri/tauri.conf.json` on main →
  builds Windows (`.exe`), macOS (`.dmg`), Linux (`.deb`) in parallel,
  publishes to GitHub Release, then mirrors assets to Cloudflare R2.
- R2 mirror: `video-plucker-releases/desktop/v<VERSION>/` prefix.
- Mirror URLs: `https://releases.xyruscode.com/desktop/v<VERSION>/<asset>`
- R2 auth: `${{ secrets.CLOUDFLARE_API_TOKEN }}` (via wrangler CLI).

**No local Rust/Tauri commands.** No `cargo build`, `cargo check`, `cargo test`,
`tauri dev`, `tauri build`. CI is the source of truth.

**Protocol:** `yt-plucker://{action}?url=...&quality=...` (action = `analyze` |
`pluck`). Registered via `tauri_plugin_deep_link`. See Integration section below.

**Features:** Single video + playlist downloads, streaming site search (AllAnime,
LuciferDonghua), quality selection (audio-only through 4K), system tray with
background downloads, resume after crash, cookie manager (import/clear
cookies.txt per profile; `vk`/`vkontakte` profile names serve vk.com + vk.ru +
vkvideo.ru URLs), YouTube cookie import from browser. Bundles the MIT-licensed
ChromeCookieUnlock yt-dlp plugin (`src-tauri/yt-dlp-plugins/`) so browser
cookies still unlock Chromium cookie DBs that are locked by a running browser.

**See also:** `Video-Plucker-Desktop/AGENTS.md` for full desktop-specific rules.

---

## Chrome Extension (`Video-Plucker-Extension/`)

**Stack:** Chrome MV3 manifest — vanilla JS service worker (`background.js`),
content script (`content.js`), popup (`popup/`). No backend — the extension
sends video URLs to the desktop app via `localhost:19877`. The repo is flat
(manifest.json at root) for direct Load unpacked in chrome://extensions.

**Key files:**
| File | Purpose |
|------|---------|
| `manifest.json` | Extension version, permissions, host patterns |
| `background.js` | Context menus, desktop pairing health check, self-update checker |
| `popup/popup.js` | Desktop status indicator, URL input, "Send to Desktop" flow |
| `content.js` | Floating "Pluck" button on supported video pages |

**Version:** `manifest.json` → `version` field.

**CI:** GitHub Actions release workflow — auto-builds and publishes on version bump.

**Desktop pairing:** The extension polls `http://localhost:19877/health` every
30 seconds. When the desktop app is running, the status dot goes green. URLs
are sent via POST to `/pair` from both the popup's "Send to Desktop" button
and the right-click context menu.

**Features:** Floating pluck button on supported sites, right-click context menu
("Pluck This Video" / "Pluck This Playlist" / "Send to Desktop App" / "Export
cookies.txt for this site"), cookies.txt export (saved to Downloads for import
into the desktop Cookie Manager), browser cookies via
`--cookies-from-browser`, playlist downloads (individual or zip).

---

## Integration Points

### Extension ↔ Desktop Protocol

The browser extension launches the desktop app via the `yt-plucker://` custom
protocol scheme.

```
yt-plucker://{action}?url={encoded_url}&quality={encoded_quality}
```

| Param | Required | Description |
|-------|----------|-------------|
| `action` | yes | `analyze` (fetch metadata) or `pluck` (download) |
| `url` | yes | URL-encoded video/page URL |
| `quality` | no | Format code (e.g. `bestvideo+bestaudio`) |

**Flow:**
1. Extension builds the protocol URL and opens it via a `data:` URL redirect
   (Chrome blocks `chrome.tabs.create` with custom schemes directly).
2. Desktop app's `tauri_plugin_single_instance` callback parses all
   `yt-plucker://` args, builds a `DeepLinkPayload`.
3. Payload is emitted as `deep-link-received` event AND stored in
   `PendingDeepLink` (`Mutex<Option<DeepLinkPayload>>`) for the startup race
   condition.
4. Frontend registers the event listener, then polls `consume_deep_link` on
   startup to catch any payload that arrived before the webview was ready.

### Extension → Desktop HTTP Pairing

When both are running, the extension can send URLs directly via HTTP:
- Health check: `GET http://localhost:19877/health`
- Send URL: `POST http://localhost:19877/pair` with `{ "url": "...", "isPlaylist": bool }`

### Protocol Spec

The canonical spec lives at
`Video-Plucker-Desktop/docs/EXTENSION_DESKTOP_INTEGRATION.md`. Both the
desktop and extension repos have a copy — keep them in sync.

---

## CI & Release Summary

| App | Workflow | Trigger | Output |
|-----|----------|---------|--------|
| Android | `android-build.yml` | push / PR | Debug APK (workflow artifact) |
| Android | `release.yml` | `v*` tag | Per-ABI APKs on GitHub Release → R2 mirror (`mobile/`) |
| Desktop | `build-check.yml` | PR | `cargo check` (no artifacts) |
| Desktop | `release.yml` | version bump on main | `.exe` / `.dmg` / `.deb` installers → R2 mirror (`desktop/`) |
| Extension | `release.yml` | version bump | Auto-build + publish |

### R2 Mirroring

All release assets are mirrored to Cloudflare R2 (bucket: `video-plucker-releases`,
custom domain: `releases.xyruscode.com`).

| App | R2 Prefix | Example URL |
|-----|-----------|-------------|
| Desktop | `desktop/v<VERSION>/` | `https://releases.xyruscode.com/desktop/v4.3.4/Video.Plucker_4.3.4_x64-setup.exe` |
| Mobile | `mobile/v<VERSION>/` | `https://releases.xyruscode.com/mobile/v4.7.4/app-universal-release.apk` |
| Extension | `extension/v<VERSION>/` | `https://releases.xyruscode.com/extension/v1.1.0/extension.zip` |

R2 auth uses `${{ secrets.CLOUDFLARE_API_TOKEN }}` passed to the `wrangler` CLI.
All uploads must use `--remote` to target the live bucket (not local simulator).

The `https://www.xyruscode.com/software` page should link to R2 URLs, not GitHub
release URLs, for all download links going forward.

---

## Repository Layout

```
Video-Plucker/
├── AGENTS.md                          ← this file
├── CLAUDE.md                          → symlink to AGENTS.md
├── Video-Plucker-Android/              ← Android app (Kotlin + Compose)
│   ├── app/build.gradle.kts           ← version + signing + ABI splits
│   ├── CLAUDE.md                      ← Android-specific AI rules
│   └── ...
├── Video-Plucker-Desktop/              ← Desktop app (Tauri v2)
│   ├── AGENTS.md                      ← Desktop-specific AI rules
│   ├── src-tauri/tauri.conf.json      ← version + deep-link config
│   ├── src-tauri/Cargo.toml           ← version + Rust deps
│   └── ...
├── Video-Plucker-Extension/            ← Chrome extension (MV3, desktop-pairing)
│   └── ...
└── Tsubarashi/                         ← temp member (Expo SDK 55 + React Native)
```

## Current Versions

| App | Version | Location |
|-----|---------|----------|
| Android | 6.4.0 (v37) | `app/build.gradle.kts` |
| Desktop | 4.4.0 | `src-tauri/tauri.conf.json` + `src-tauri/Cargo.toml` |
| Extension | 1.2.5 | `manifest.json` |
| Tsubarashi | 0.1.1 | `Tsubarashi/app.json` + `Tsubarashi/package.json` |
