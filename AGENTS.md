# Video Plucker — Repository Instructions

## Build & Verify via CI, not locally

- **We do not run builds or tests locally.** No `cargo build`, `cargo check`, `cargo test`, `tauri dev/build`, `dev.ps1`, or `build.ps1` for verification. The local GNU toolchain setup is fragile (see `dev.ps1`) and CI is the source of truth.
- After making changes: commit, push to the feature branch, open the PR, then **watch CI on GitHub** (`gh pr checks --watch`).
- **When CI fails:** use the **rinse-and-repeat** skill (`/rinse-and-repeat`): it opens the failed job log, fixes the root cause, pushes, and polls CI until green.
- CI runs the `Build Check` workflow (`build-check.yml`): `cargo check` on `windows-latest` for every PR. It does not run unit tests; if you add tests, call them out in the PR for a manual CI step if needed.
- Once CI is green, follow the global workflow: merge manually with a regular merge commit via `gh pr merge` flow (no squash/auto-merge).

## Conventions

- Version is maintained in **both** `src-tauri/tauri.conf.json` and `src-tauri/Cargo.toml` (keep them in sync; bump both per release).
- Feature commits carry the version bump, e.g. `feat: ... (v4.4.0)`.
- Protocol contract with the browser extension lives in `docs/EXTENSION_DESKTOP_INTEGRATION.md` — update the version table when the protocol or app version changes.

## Release & R2 Mirroring

- **Trigger:** Bump `version` in `src-tauri/tauri.conf.json` and land on `main`. The `release.yml` workflow auto-detects whether a release for that version already exists (idempotent; no duplicate releases).
- **Build:** Windows (`.exe`), macOS (`.dmg`), Linux (`.deb`) in parallel via tauri-action.
- **R2 mirror:** After GitHub Release creation, the `mirror` job downloads all assets and uploads them to Cloudflare R2 bucket `video-plucker-releases` under `desktop/v<VERSION>/`.
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