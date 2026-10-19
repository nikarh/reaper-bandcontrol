; Windows installer for the REAPER extension, built by scripts/package-windows.sh:
;
;   makensis -DVERSION=0.1.0 -DARCH=x64 -DDLL=path\to\reaper_bandcontrol.dll -DOUT=setup.exe [-DSIGN=command] bandcontrol.nsi
;
; Installs for the current user (no administrator rights) into REAPER's resource folder, which defaults to
; %APPDATA%\REAPER and can be changed for portable installs. SIGN, when given, is run on the installer and the
; uninstaller with the file name appended.

Unicode true
SetCompressor /SOLID lzma
RequestExecutionLevel user

!include "MUI2.nsh"
!include "LogicLib.nsh"

!define NAME "BandControl"
!define UNINSTALL_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\BandControl"

Name "${NAME} ${VERSION} for REAPER (${ARCH})"
OutFile "${OUT}"
InstallDir "$APPDATA\REAPER"
InstallDirRegKey HKCU "${UNINSTALL_KEY}" "InstallLocation"
BrandingText "${NAME} ${VERSION}"

!ifdef SIGN
  !finalize '${SIGN} "%1"'
  !uninstfinalize '${SIGN} "%1"'
!endif

!define MUI_ABORTWARNING
!define MUI_WELCOMEPAGE_TEXT "This installs the ${NAME} extension into REAPER, so phones can control playback and in-ear mixes over Bluetooth.$\r$\n$\r$\nClose REAPER before you continue."
!insertmacro MUI_PAGE_WELCOME
!define MUI_DIRECTORYPAGE_TEXT_TOP "The REAPER resource folder: %APPDATA%\REAPER for a normal installation, the REAPER folder itself for a portable one. REAPER shows it under Options → Show REAPER resource path in explorer/finder."
!define MUI_DIRECTORYPAGE_TEXT_DESTINATION "REAPER resource folder"
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!define MUI_FINISHPAGE_TEXT "Start REAPER and run the action $\"BandControl: Show pairing QR code$\" to pair the first phone."
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "English"

Function .onVerifyInstDir
  ; Anything works for a portable REAPER, but an empty path doesn't.
  ${If} $INSTDIR == ""
    Abort
  ${EndIf}
FunctionEnd

Section "BandControl"
  SetOutPath "$INSTDIR\UserPlugins"
  ; A running REAPER keeps the library open: NSIS asks to retry until it is closed.
  SetOverwrite on
  File "/oname=reaper_bandcontrol.dll" "${DLL}"
  WriteUninstaller "$INSTDIR\UserPlugins\BandControl-uninstall.exe"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayName" "${NAME} for REAPER"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayVersion" "${VERSION}"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "Publisher" "${NAME}"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "UninstallString" '"$INSTDIR\UserPlugins\BandControl-uninstall.exe"'
  WriteRegDWORD HKCU "${UNINSTALL_KEY}" "NoModify" 1
  WriteRegDWORD HKCU "${UNINSTALL_KEY}" "NoRepair" 1
SectionEnd

Section "Uninstall"
  ; The uninstaller lives in UserPlugins; the pairing code and name stay in reaper-extstate.ini.
  Delete "$INSTDIR\reaper_bandcontrol.dll"
  Delete "$INSTDIR\BandControl-uninstall.exe"
  DeleteRegKey HKCU "${UNINSTALL_KEY}"
SectionEnd
