; NSIS installer for the Rune build of Video Plucker.
; Invoked with: makensis /DVERSION=x.y.z /DBIN=<staging dir> /DOUT=<setup.exe> /DICON=<icon.ico> scripts/installer.nsi

!ifndef ICON
  !define ICON "..\src-tauri\icons\icon.ico"
!endif

Name "Video Plucker"
OutFile "${OUT}"
InstallDir "$LOCALAPPDATA\Programs\Video Plucker"
InstallDirRegKey HKCU "Software\Video Plucker" "InstallDir"
RequestExecutionLevel user
ShowInstDetails show
Icon "${ICON}"
UninstallIcon "${ICON}"

Section "Install"
  SetOutPath "$INSTDIR"
  File /r "${BIN}\*.*"

  ; yt-plucker:// deep-link registration (the extension protocol).
  WriteRegStr HKCU "Software\Classes\yt-plucker" "" "URL:Video Plucker"
  WriteRegStr HKCU "Software\Classes\yt-plucker" "URL Protocol" ""
  WriteRegStr HKCU "Software\Classes\yt-plucker\shell\open\command" "" '"$INSTDIR\yt-plucker.exe" "%1"'

  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Video Plucker" "DisplayName" "Video Plucker"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Video Plucker" "UninstallString" '"$INSTDIR\uninstall.exe"'
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Video Plucker" "DisplayVersion" "${VERSION}"
  WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Video Plucker" "NoModify" 1
  WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Video Plucker" "NoRepair" 1
  WriteUninstaller "$INSTDIR\uninstall.exe"
SectionEnd

Section "Uninstall"
  DeleteRegKey HKCU "Software\Classes\yt-plucker"
  DeleteRegKey HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Video Plucker"
  RMDir /r "$INSTDIR"
SectionEnd
