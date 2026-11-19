; Windfall installer: per-user (no administrator needed), starts with Windows, can be uninstalled from Settings.
Unicode true
!define NAME "Windfall"
!ifndef VERSION
  !define VERSION "0.0.0"
!endif
Name "${NAME}"
OutFile "..\dist\Windfall-Setup.exe"
InstallDir "$LOCALAPPDATA\Programs\Windfall"
RequestExecutionLevel user
SetCompressor /SOLID lzma

Page directory
Page instfiles
UninstPage uninstConfirm
UninstPage instfiles

Section "Install"
  SetOutPath "$INSTDIR"
  ; stop a running copy so its files can be replaced
  nsExec::Exec '"$INSTDIR\windfallctl.exe" quit'
  Sleep 800
  File "..\dist\windfalld.exe"
  File "..\dist\windfallctl.exe"
  WriteUninstaller "$INSTDIR\Uninstall.exe"
  CreateDirectory "$SMPROGRAMS\Windfall"
  CreateShortcut "$SMPROGRAMS\Windfall\Windfall.lnk" "$INSTDIR\windfalld.exe"
  CreateShortcut "$SMPROGRAMS\Windfall\Windfall Controller.lnk" "$INSTDIR\windfalld.exe" "--controller"
  ; start with Windows (the tray's "Start with Windows" switch can turn this off again)
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "Windfall" '"$INSTDIR\windfalld.exe"'
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Windfall" "DisplayName" "Windfall"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Windfall" "DisplayVersion" "${VERSION}"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Windfall" "Publisher" "tak0dan"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Windfall" "UninstallString" '"$INSTDIR\Uninstall.exe"'
  Exec '"$INSTDIR\windfalld.exe"'
SectionEnd

Section "Uninstall"
  nsExec::Exec '"$INSTDIR\windfallctl.exe" quit'
  Sleep 800
  Delete "$INSTDIR\windfalld.exe"
  Delete "$INSTDIR\windfallctl.exe"
  Delete "$INSTDIR\Uninstall.exe"
  RMDir "$INSTDIR"
  Delete "$SMPROGRAMS\Windfall\Windfall.lnk"
  Delete "$SMPROGRAMS\Windfall\Windfall Controller.lnk"
  RMDir "$SMPROGRAMS\Windfall"
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "Windfall"
  DeleteRegKey HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Windfall"
SectionEnd
