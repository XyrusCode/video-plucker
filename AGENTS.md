# Video Plucker — Repository Instructions

## Build & Verify via CI, not locally

- **We do not run builds or tests locally.** No `cargo build`, `cargo check`, `cargo test`, or `cargo run` for verification. The local GNU toolchain is fragile and CI is the source of truth. This includes Rune's headless `--snap`/`--tree` checks: they run in CI, not on this machine.
- After making changes: commit, push to the feature branch, open the PR, then **watch CI on GitHub** (`gh pr checks --watch`).
- **When CI fails:** use the **rinse-and-repeat** skill (`/rinse-and-repeat`): it opens the failed job log, fixes the root cause, pushes, and polls CI until green.
- CI runs `Build Check` (`.github/workflows/build-check.yml`): `cargo check` + `cargo test` on `windows-latest`, then a `rune-ui` job that renders every seeded scene headlessly (`--snap` PNG + `--tree` JSON) and uploads the snapshots as artifacts. Review those artifacts when reviewing a UI change.
- Once CI is green, follow the global workflow: merge manually with a regular merge commit via `gh pr merge` flow (no squash/auto-merge).

## Architecture

- The UI is **Rune** (`rune` + `rune-kit`), a native GPU-rendered Rust UI framework. There is no webview, no Tauri, no npm dependency and no build step for the frontend.
- Rune is pre-release and pinned to a `dev` revision in the **workspace root** `Cargo.toml`, together with all eight `[patch.crates-io]` entries it requires. When bumping the rev, change Rune and all patches to the same rev.
- The download engine (yt-dlp + ffmpeg), extractors, ledger, history, backup dumps and retry/failure classification live in `src-tauri/src/` and are plain Rust. `compat.rs` supplies the small process/event/path shim the old Tauri commands used; `ui.rs` is the state, action and view layer.
- Cross-app contracts are unchanged: `yt-plucker://` deep links, version-3 dumps and the ledger/history JSON formats. There is no HTTP pairing server in this codebase.

## Conventions

- Version lives in **`src-tauri/Cargo.toml`** only (there is no `tauri.conf.json`). Feature commits carry the version bump, e.g. `feat: ... (v6.0.0)`.
- Protocol contract with the browser extension lives in `docs/EXTENSION_DESKTOP_INTEGRATION.md` — update the version table when the protocol or app version changes.

## Release & R2 Mirroring

- **Trigger:** Bump `version` in `src-tauri/Cargo.toml` and land on `main`. `release.yml` rejects versions below the latest release and skips versions already released.
- **Build:** `cargo build --release` per platform, then custom packaging: Windows NSIS `.exe`, macOS `.dmg`, Linux `.deb`. Each installer bundles yt-dlp, ffmpeg and `yt-dlp-plugins/` next to the executable and registers the `yt-plucker` scheme.
- **Updater:** `publish` produces `latest.json` with a SHA-256 checksum per platform; the in-app updater downloads, verifies and launches the installer.
- **R2 mirror:** After GitHub Release creation, the `mirror` job uploads all assets to Cloudflare R2 bucket `video-plucker-releases` under `desktop/v<VERSION>/`.
- **Mirror URLs:** `https://releases.xyruscode.com/desktop/v<VERSION>/<asset>`
- **Auth:** `${{ secrets.CLOUDFLARE_API_TOKEN }}` passed to `wrangler r2 object put` with `--remote` flag.
- **Manual mirror (if CI is stuck):** `npx wrangler r2 object put "video-plucker-releases/desktop/v<VERSION>/<file>" --file <path> --remote`

## Pending: AllAnime Desktop Fix

AllAnime's episode source API now requires an `aaReq` crypto token (AES-GCM encrypted payload). The current `allanime.rs` extractor returns `AA_CRYPTO_MISSING` errors.

**Potential fix:** Port the approach from [ani-cli-rs](https://github.com/vorlie/ani-cli-rs), which uses **Anikoto API + MegaPlay** instead of AllAnime directly:
- Anikoto API: `https://anikotoapi.site/` (clean REST, no crypto)
- MegaPlay: `https://megaplay.buzz/` (stream extraction)
- No aaReq token required
- AniList GraphQL for search: `https://graphql.anilist.co`

**Implementation notes for later:**
1. Search via AniList GraphQL (existing AllAnime search can be repurposed)
2. Episodes via `GET anikotoapi.site/series/{id}`
3. Stream via MegaPlay embed: fetch HTML → extract `data-id` → `GET megaplay.buzz/stream/getSources?id={dataId}`
4. Required headers: `Referer: https://megaplay.buzz/`, `Origin: https://megaplay.buzz`
5. KotoCDN domains: `megaplay.buzz`, `mewstream.buzz`, `kotocdn.site`