# Xyrus' YT Plucker

A desktop app for plucking videos from YouTube and X (Twitter), built with
[Tauri v2](https://v2.tauri.app) (Rust backend, vanilla HTML/CSS/JS frontend).
Plucking is powered by bundled [yt-dlp](https://github.com/yt-dlp/yt-dlp)
and [ffmpeg](https://github.com/BtbN/FFmpeg-Builds) sidecar binaries, so there
is nothing extra to install. (yt-dlp handles many other sites too, so most
video URLs it supports will work.)

## Features

- Pluck a single video or an entire playlist from one URL — YouTube videos and
  playlists, and X (Twitter) videos
- Quality selection: best, 2160p, 1440p, 1080p, 720p, 480p, or audio-only
  (MP3 / M4A)
- Live progress with percent, speed, and ETA (per-item and overall for
  playlists)
- Expand a playlist pluck to see every video as its own row with individual
  status and progress
- Resume after a crash: plucks survive an app or network failure. yt-dlp keeps
  partial files and a per-pluck archive of finished items, so resuming
  continues where it stopped instead of starting over. Interrupted plucks
  reappear on the next launch with a Resume button
- Minimize or close to the system tray with plucks still running in the
  background; restore from the tray, or Show/Quit from its menu
- Cancel an in-progress pluck (terminates the whole yt-dlp/ffmpeg tree)
- Remembers your destination folder and quality choice between runs

## Install

Grab the file for your platform from the [Releases](../../releases) page:

- Windows: `.exe` installer — installs to `Program Files` (prompts for
  administrator), and creates Start Menu and Desktop shortcuts
- macOS: `.dmg` (unsigned; on first launch, right-click the app and choose
  Open to get past Gatekeeper)
- Linux: `.deb` (Debian/Ubuntu)

yt-dlp and ffmpeg are bundled inside the app, so there is nothing else to
install.

## Usage

1. Paste a YouTube video or playlist URL and click Analyze.
2. Review the title, thumbnail, and available qualities (playlists show the
   entry count).
3. Pick a quality and a destination folder.
4. Click Pluck. Progress, speed, and ETA update live; use Cancel to stop, or
   Open folder when a pluck finishes.

Closing or minimizing the window sends it to the system tray and plucks keep
running. Click the tray icon to restore, or use its Show / Quit menu.

## Building from source

### Prerequisites

- [Rust](https://rustup.rs) (stable, MSVC toolchain) with the Visual Studio
  Build Tools "Desktop development with C++" workload
- [Node.js](https://nodejs.org) and the Tauri CLI
  (`npm install -g @tauri-apps/cli`)
- WebView2 runtime (preinstalled on Windows 11)

### Sidecar binaries

The `yt-dlp` and `ffmpeg` binaries are not committed to the repository.
Download them into `src-tauri/binaries/` with the target-triple filenames Tauri
expects for your platform. On Windows:

```powershell
New-Item -ItemType Directory -Force src-tauri\binaries

# yt-dlp
Invoke-WebRequest `
  -Uri "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp.exe" `
  -OutFile "src-tauri\binaries\yt-dlp-x86_64-pc-windows-msvc.exe"

# ffmpeg (BtbN static build, LGPL variant)
Invoke-WebRequest `
  -Uri "https://github.com/BtbN/FFmpeg-Builds/releases/latest/download/ffmpeg-master-latest-win64-lgpl.zip" `
  -OutFile "$env:TEMP\ffmpeg.zip"
Expand-Archive "$env:TEMP\ffmpeg.zip" -DestinationPath "$env:TEMP\ffmpeg" -Force
Copy-Item (Get-ChildItem "$env:TEMP\ffmpeg" -Recurse -Filter ffmpeg.exe).FullName `
  "src-tauri\binaries\ffmpeg-x86_64-pc-windows-msvc.exe"
```

The [release workflow](.github/workflows/release.yml) shows the equivalent
commands for macOS and Linux. The LGPL ffmpeg build is used deliberately: it
covers everything the app needs (mp4 muxing, mp3 via libmp3lame) without GPL
redistribution obligations.

### Develop and build

```powershell
tauri dev      # run with a hot-reload window
tauri build    # produce the platform installer under src-tauri\target\release\bundle
```

## Releases

Pushing a `v*` tag (for example `v1.0.1`) triggers the
[release workflow](.github/workflows/release.yml), which builds the app in
parallel on Windows, macOS, and Linux runners and publishes the installers
(`.exe`, `.dmg`, `.deb`) to a single GitHub Release. Each runner downloads its
own platform's sidecar binaries automatically, so they never need to be
committed.

The macOS build targets Intel (x86_64) and runs on Apple Silicon through
Rosetta 2. The Linux build targets x86_64.

## Notes

- YouTube periodically breaks yt-dlp extractors. If plucks start failing,
  update the bundled `yt-dlp` to the latest release and rebuild.
- The bundled ffmpeg is a static LGPL build.
