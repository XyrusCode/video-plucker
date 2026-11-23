#!/bin/sh
set -e
# Tauri names the desktop file after productName ("Video Plucker.desktop"),
# but freedesktop desktop-entry IDs cannot contain spaces, so GNOME/KDE
# ignore it. Rename to a valid ID while keeping display name inside.
OLD="/usr/share/applications/Video Plucker.desktop"
NEW="/usr/share/applications/video-plucker.desktop"
if [ -f "$OLD" ]; then
  mv -f "$OLD" "$NEW"
fi
