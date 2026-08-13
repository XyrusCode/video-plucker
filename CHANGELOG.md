# Changelog

All notable changes to Video Plucker (Desktop) are documented in this file.
The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [4.5.0] - 2026-08-13

### Added

- Download queueing: downloads now run one at a time in first-in, first-out
  order instead of all at once.
- Pause and resume: pause an in-progress download and pick it back up later.
  Partial progress is kept, and already-finished items in a playlist are skipped
  on resume.
- Links to the XyrusCode software catalogue and to the Discord community, in the app footer.

### Changed
- Queue now optional: users can toggle queue on/off in Settings → Experimental features
- 4-tab structure: Download, Queue, History, Settings (Browser optional 5th tab)
- Download button shows "Add to Queue" or "Download" based on queue setting
