# Changelog

All notable changes to Video Plucker (Desktop) are documented in this file.
The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [4.14.0] - 2026-10-19

### Added
- **A Saved tab.** The videos you saved on YouTube, X, TikTok and VK, read with the cookies you
  already imported for those sites. One tile per video, with the site, the author and a Pluck
  button. A site you have no login for is never read and never offered, so the filter cannot lead
  to an empty list that looks like a bug. One site refusing does not hide the others.
- **YouTube lists.** Watch Later, Liked videos, and your own playlists, when YouTube is
  connected.

### Notes
- Read-only, on purpose. Removing a video from these lists means calling each site's private
  endpoint, and every one of them answers with success whether or not it did anything, so a
  removal cannot be confirmed without reading the whole list again.
- The HttpOnly cookies these sites need are read out of the imported cookies.txt. They look like
  comments in that file, and a reader that skips comments skips exactly the cookies that matter.
- The version is 4.14.0 and not 4.12.0: 4.10.0, 4.12.0 and 4.13.0 are already taken by the
  releases of the old numbering, and the release workflow skips a version that has a release.

## [4.11.0] - 2026-10-19

### Added
- **Several links at once.** The link field takes one URL per line. One link is analysed into
  the detail card, as before. Several links are analysed into a grid of tiles, a few at a time,
  each carrying the thumbnail, the title, the site and the length.
- **A link that could not be fetched can be fetched again.** The tile says what went wrong in
  its own thumbnail, with a Refetch button on it.
- **Pluck all** starts every tile in the grid at once, through the same queue as everything
  else, so the app still runs one download at a time unless it is told otherwise.

### Changed
- **Plucking empties the field.** The link is in the queue now, so the field is empty again.
  In a batch, the plucked tile leaves the grid for the same reason.

## [4.10.0] - 2026-10-19

### Added
- **A failed download can be retried.** The row's primary action is now Retry. It runs the same
  download again, keeping the yt-dlp archive, so items that already finished are not downloaded a
  second time, and picking up a cookies.txt imported since the failure.
- **A dropped connection is retried by itself.** A transport failure (the network went away, the
  socket timed out, the origin answered 5xx) is run again up to three times, 2s then 5s then 12s
  apart, and the row says so while it waits. Forward progress puts the budget back, so a long
  download over a flapping connection never gives up. A login gate, a photo post and a missing
  page are not retried: running them again gets the same answer.

### Changed
- Failure text is now plain about which kind of failure it was. A transport failure says the
  network dropped instead of "Download failed."

## [4.9.1] - 2026-10-19

### Fixed
- **A cleared download took its failure text with it.** The failure notice is a sibling row of
  the download row. Removing the download row left the notice behind, so "Download failed."
  stayed on screen with nothing behind it. Clearing a download, removing it from the list, or
  parking it for later now takes the notice with it.

## [4.9.0] - 2026-10-19

### Changed
- **The desktop app is a desktop app.** The 4.8.0 mirror of the phone layout is replaced by a
  workbench: a left rail for the four destinations, Downloads and Queue as tables with a sticky
  header, Settings as two panes, and a status bar that reports engine state, active count,
  throughput, the destination folder and the version.
- **Frameless window on Windows.** The title bar carries the brand and the minimise, maximise
  and close controls. Close still hides to the tray. macOS and Linux keep their native
  decorations.
- **`DESIGN.md` is independent.** It is no longer a mapping of the Android contract. Desktop and
  Android now each own their own design; they share only the name, logo, accent hue and dump
  format.
- **Two reds, on purpose.** `--accent` (`#FF2D55`) marks, `--accent-fill` (`#E01E45`) carries
  white text, because white on the brand red is 3.64:1 and fails AA.
- **Status is a word in a fixed column**, not a shape, and no text is uppercase.

### Fixed
- Failure text uses its own red, so a failure never reads as a button.
- Icons are drawn SVG in one stroke weight, not Unicode glyphs.
## [4.8.0] - 2026-10-18

### Added
- **Backup & Restore.** Settings can be exported to, and imported from, the same dump file the
  Android app writes. A dump moves between either machine unchanged: settings, per-site login
  cookies and the parked queue. The file contains live session cookies, so keep it private.
- **UI contract.** `DESIGN.md` pins the colour, spacing, typography, component and motion values
  this app and the Android app are both held to, so the next visual change is made once instead
  of twice. The Android repository holds the canonical copy; this repo documents how each rule
  lands in CSS.
- **UI smoke tests.** The frontend now has a test suite. It asserts the design tokens match the
  contract, that every element id `main.js` looks up on boot actually exists, and that
  downloads stay inside the Pluck tab. Wired into CI, which never ran the existing Rust tests
  either.

### Changed
- **The app is now the phone's colour.** The accent was Apple blue throughout, with a pink
  gradient hardcoded on the logo square as the only brand colour in the app. Both are gone; the
  accent is a single token now. Layout follows the shared contract too: a brand top bar, pill
  navigation, flat rows instead of translucent cards, section headers with an accent dot, and a
  Material toggle instead of a bare checkbox.
- **Downloads moved under the Pluck tab.** They sat outside the view system, so the list
  rendered on the Search and Settings tabs as well.

### Fixed
- **A cookie-manager failure is visible wherever you are.** Errors were written into the Pluck
  tab's error line, which is not rendered on any other tab.
- **"Start all" starts downloads in the order the list shows.** It iterated the store in
  insertion order while the list was sorted oldest-first.
- **A download card no longer overflows.** Its controls were laid out in a row with no wrapping,
  which broke past three visible buttons.
- **"Start all" no longer turns red on hover.** It was borrowing the styling of the destructive
  "Clear finished" button.
- **The app no longer renders a blur that blurs nothing.** Every panel asked for a backdrop
  filter over a flat background with nothing behind it, at the cost of a compositing layer.

## [4.5.1] - 2026-08-23

### Fixed

- Browser-cookie downloads no longer fail with "Could not copy Chrome cookie
  database" while Chrome is running: the bundled ChromeCookieUnlock yt-dlp
  plugin is now actually loaded (it shipped since v4.4.0 but was never passed
  to yt-dlp via `--plugin-dirs`).
- Cookie profiles saved as "x.com" now serve twitter.com links (and vice
  versa), matching how the sites share logins.
- The cookie-copy failure now shows plain-language guidance instead of the
  raw error.

### Note

Version numbering continues from the last feature release line at the
maintainer's request; v4.10–v4.13 releases contained no functional changes
over v4.5.0.

## [4.13.0] - 2026-08-22

### Added

- Queue for later: failed downloads can be parked on a dedicated Queue tab
  and restarted whenever you like — nothing auto-starts, and parked items
  survive app restarts.
- Report button on failed downloads opens a pre-filled GitHub issue, with
  automatic duplicate detection pointing you at an existing report when one
  already covers the same failure.

### Changed

- Failed downloads now show a plain-language explanation instead of raw
  yt-dlp output (e.g. login-required sites point at the Cookie Manager).
  Technical details still travel with every issue report.
- Self-healing downloader: when a site breaks because the bundled yt-dlp got
  outdated, Video Plucker updates it automatically in the background and
  retries once.
- AllAnime is greyed out in Search while broken upstream; LuciferDonghua
  remains available.

### Fixed

- In-app updater now verifies update signatures correctly (the embedded
  public key was a placeholder, which made silent updates fail).

## [4.5.0] - 2026-08-13

### Added

- Download queueing: downloads now run one at a time in first-in, first-out
  order instead of all at once.
- Pause and resume: pause an in-progress download and pick it back up later.
  Partial progress is kept, and already-finished items in a playlist are skipped
  on resume.
- Links to the XyrusCode software catalogue and to the Discord community, in the
  app footer.
