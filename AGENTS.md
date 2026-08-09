# Xyrus YT Plucker — Repository Instructions

## Build & Verify via CI, not locally

- **We do not run builds or tests locally.** No `cargo build`, `cargo check`, `cargo test`, `tauri dev/build`, `dev.ps1`, or `build.ps1` for verification. The local GNU toolchain setup is fragile (see `dev.ps1`) and CI is the source of truth.
- After making changes: commit, push to the feature branch, open the PR, then **watch CI on GitHub** (`gh pr checks --watch`).
- **When CI fails:** use the **rinse-and-repeat** skill (`/rinse-and-repeat`): it opens the failed job log, fixes the root cause, pushes, and polls CI until green.
- CI runs the `Build Check` workflow (`build-check.yml`): `cargo check` on `windows-latest` for every PR. It does not run unit tests; if you add tests, call them out in the PR for a manual CI step if needed.
- Once CI is green, follow the global workflow: merge manually with a regular merge commit via `gh pr merge` flow (no squash/auto-merge).

## Conventions

- Version is maintained in **both** `src-tauri/tauri.conf.json` and `src-tauri/Cargo.toml` (keep them in sync; bump both per release).
- Feature commits carry the version bump, e.g. `feat: ... (v4.2.0)`.
- Protocol contract with the browser extension lives in `docs/EXTENSION_DESKTOP_INTEGRATION.md` — update the version table when the protocol or app version changes.