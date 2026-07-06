# yt-grab

A Windows desktop app for downloading YouTube videos and playlists, built with
[Tauri v2](https://v2.tauri.app) (Rust backend, vanilla HTML/CSS/JS frontend).
Downloads are powered by bundled [yt-dlp](https://github.com/yt-dlp/yt-dlp)
and [ffmpeg](https://github.com/BtbN/FFmpeg-Builds) sidecar binaries, so there
is nothing extra to install.

## Features

- Download a single video or an entire playlist from one URL
- Quality selection: best, 2160p, 1440p, 1080p, 720p, 480p, or audio-only
  (MP3 / M4A)
- Live progress with percent, speed, and ETA (per-item and overall for
  playlists)
- Minimize or close to the system tray with downloads still running in the
  background; restore from the tray, or Show/Quit from its menu
- Cancel an in-progress download (terminates the whole yt-dlp/ffmpeg tree)
- Remembers your download folder and quality choice between runs

## Install

Grab the latest `yt-grab_x.y.z_x64-setup.exe` from the
[Releases](../../releases) page and run it. It is a per-user install and does
not require administrator rights. `yt-dlp.exe` and `ffmpeg.exe` are bundled
inside the app.

## Building from source

### Prerequisites

- [Rust](https://rustup.rs) (stable, MSVC toolchain) with the Visual Studio
  Build Tools "Desktop development with C++" workload
- [Node.js](https://nodejs.org) and the Tauri CLI
  (`npm install -g @tauri-apps/cli`)
- WebView2 runtime (preinstalled on Windows 11)

### Sidecar binaries

The `yt-dlp.exe` and `ffmpeg.exe` binaries are not committed to the repository.
Download them into `src-tauri/binaries/` with the target-triple filenames Tauri
expects:

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

The LGPL ffmpeg build is used deliberately: it covers everything the app needs
(mp4 muxing, mp3 via libmp3lame) without GPL redistribution obligations.

### Develop and build

```powershell
tauri dev      # run with a hot-reload window
tauri build    # produce the NSIS installer under src-tauri\target\release\bundle\nsis
```

## Releases

Pushing a `v*` tag (for example `v1.0.0`) triggers the
[release workflow](.github/workflows/release.yml), which builds the app on a
Windows runner and publishes the installer to a GitHub Release. The workflow
downloads the sidecar binaries automatically, so they never need to be
committed.

## Notes

- YouTube periodically breaks yt-dlp extractors. If downloads start failing,
  update the bundled `yt-dlp.exe` to the latest release and rebuild.
- The bundled ffmpeg is a static LGPL build from BtbN's FFmpeg-Builds.
