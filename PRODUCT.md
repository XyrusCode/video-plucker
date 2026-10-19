# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

The frontend is HTML, CSS and JavaScript rendered in a native window (Tauri v2).
Recorded as `web` because that is the technology; the target is a desktop
application window, never a browser page and never a phone. Nothing in the
interface may assume a mobile viewport, a touch target, or a narrow column.

## Stack

Tauri v2. Rust backend, vanilla HTML/CSS/JS frontend, no build step, no bundler,
no framework, no npm dependency in the shipped app. yt-dlp and ffmpeg run as
bundled sidecar binaries. The Playwright harness in `tests/` is test-only and is
not bundled into the binary.

## Users

People who already know what they want to save and where they want it. They are
not searching for content, they are filing something they have already found,
usually a video link that some other tool could not hand them as a file.

The common situations, in the order the app meets them:

- A link failed somewhere else, or arrived without a file, and they want the file.
- A YouTube "confirm you're not a bot" wall, which a logged-in browser profile
  gets past and an anonymous downloader does not.
- A playlist or a series, where they care about per-item status rather than one
  aggregate number.
- Age-restricted or private content, which needs their own cookies imported.

They run this alongside other work, and they leave it running. The app spends
most of its life in the background with active downloads, restored on next
launch after a crash or a network drop.

## Product Purpose

Turn a video URL into a file on the user's disk, at the quality they chose, in
the folder they chose, and tell them honestly what is happening while it runs.

Success is a file that plays. Everything else in the app is in service of that:
the analysis step exists to avoid a wrong-format download, the queue exists so a
transient failure does not cost the work, and the cookies exist because some
sites refuse anonymous access.

## Positioning

A local file-producing tool, not a player, not a browser, not a streaming
service. It has no catalogue of its own, no discovery surface beyond the
streaming-site search the user explicitly opens, and no playback. It runs
yt-dlp locally and hands back what lands on disk.

The mechanism a neighbouring app could not copy truthfully: the same dump file
that carries settings, cookies and the parked queue between this app and the
Android app, so a machine swap does not cost the user their logins or their
queue.

## Operating Context

- Minimising or closing sends the window to the system tray; downloads continue.
  The tray icon restores it, or offers Show / Quit.
- A download can outlive the app. On the next launch it reappears as an
  interrupted row with Resume, because yt-dlp keeps partial files and an archive
  of finished items.
- Downloads run one at a time by default, several at a time in parallel mode.
- A failed download can be parked in the Queue and started again later, which is
  what a user does when they are waiting on an app update that fixes whatever
  broke.
- Destination folder and quality are remembered between runs.
- Per-site cookie sets are named by a substring that matches URLs, so `x` covers
  both x.com and twitter.com. A name is sanitised before it reaches disk.

## Capabilities and Constraints

Confirmed functionality:

- Pluck a single video or a whole playlist from one URL (YouTube, X, and other
  yt-dlp sites).
- Pluck several links at once: paste them one per line, and each is analysed
  into a tile of a grid, with the ones that could not be read offering to be
  fetched again.
- Search a streaming-site catalogue, then download a movie directly or
  batch-select a series' episodes by range or checkbox.
- Quality selection: best, 2160p, 1440p, 1080p, 720p, 480p, audio-only MP3/M4A.
- Live progress with percent, speed and ETA, overall and per playlist item.
- Pause, resume, cancel (terminates the whole yt-dlp/ffmpeg tree), park, report.
- Site cookies: import a cookies.txt per site, clear one, delete one.
- Export and import a settings/cookies/queue dump shared with the Android app.
- Check for app updates; updater artifacts are signed.

Technical constraints that bear on the interface:

- No build step. The frontend ships as source files. This rules out every
  technique that needs compiling, and it rules out shipping a dependency.
- The frontend cannot know the platform it is running on without asking, and
  window behaviour differs per platform.
- yt-dlp extractors break upstream. Any failure UI must survive a changed
  failure reason, which is why failures carry a friendly verdict plus raw lines
  kept only for issue reports.
- AllAnime's API requires a crypto token the app does not have; that extractor
  is known broken and is listed as unavailable in the site picker rather than
  silently failing at search time.

Undecided: AllAnime's replacement extractor is scoped but not implemented.

## Brand Commitments

- Name: Video Plucker. "Pluck" is the product's verb and appears in the app.
- Logo mark: a rounded square holding a play triangle. It is the brand.
- Accent: `#FF2D55`. Confirmed by the user as the single accent for the desktop
  app, kept from the Android app so the two products stay recognisably related.
- Binding visual references, named by the user: Docker Desktop (the one they
  singled out), IObit Uninstaller, and MiniTool Partition Wizard.
- The user rejected the previous direction explicitly: mirroring the Android
  phone's layout on the desktop. A phone layout in a desktop window is the
  anti-pattern, not the target.
- Voice: plain and specific. Controls name their action ("Check for updates",
  not "Submit"). Errors name the problem and the recovery.
- Windows is the primary target platform; macOS and Linux are shipped too.

## Evidence on Hand

- Real app: `src/index.html`, `src/styles.css`, `src/main.js`, Rust in
  `src-tauri/src/`.
- Screenshots of the incumbent: `docs/ui/pluck-before.png` and the other
  `docs/ui/*-before.png` files, plus `docs/analyze.png` and `docs/playlist.png`.
- Committed before/after pairs from the 4.8.0 restyle, shot through the same
  stub and viewport by `scripts/shoot-before-after.mjs`.
- Real streaming-site posters and thumbnails at runtime.

Absences future work must not fabricate: no testimonials, no customer logos, no
usage or benchmark numbers, no download counts, no uptime or performance
claims. If a design needs a number the app does not produce, the number comes
from the running app or it does not appear.

## Product Principles

1. **The file is the product.** Every screen ends at a file on disk. Do not add a
   step that does not move the user closer to one.
2. **Honesty about state.** A download that is paused says paused. A failure
   names the cause in plain language and offers the recovery. Never report
   success for work that did not complete.
3. **The user's logins are theirs.** Cookies are stored locally, exported only on
   request, and a dump is labelled as containing live session credentials.
4. **Background work is normal.** The app runs while the user does something else,
   and comes back to exactly the state it left after a crash.
5. **Restraint earns trust.** This is a file-producing utility that sits open all
   day. Nothing earns its place through decoration.

## Accessibility & Inclusion

- Every control is reachable and operable by keyboard, with a visible focus
  indicator. `prefers-reduced-motion` is honoured.
- Body and placeholder text meet 4.5:1 against their surface; large text meets
  3:1. Colour never carries meaning alone, so status is always text plus colour.
- The window must remain usable at the enforced minimum size and at 125% and
  150% display scaling.