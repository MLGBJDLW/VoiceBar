; Native per-user installer. No browser runtime, network bootstrap or elevation.
Unicode true
ManifestDPIAware true
RequestExecutionLevel user
SetCompressor /SOLID lzma
!include "MUI2.nsh"
!include "FileFunc.nsh"
!include "LogicLib.nsh"
!include "x64.nsh"

!ifndef APP_VERSION
  !error "APP_VERSION is required"
!endif
!ifndef FILE_VERSION
  !error "FILE_VERSION is required (four numeric parts)"
!endif
!ifndef PAYLOAD_DIR
  !error "PAYLOAD_DIR is required"
!endif
!ifndef OUTPUT_FILE
  !error "OUTPUT_FILE is required"
!endif

Name "VoiceBar"
OutFile "${OUTPUT_FILE}"
InstallDir "$LOCALAPPDATA\VoiceBar"
InstallDirRegKey HKCU "Software\VoiceBar" "InstallDir"
VIProductVersion "${FILE_VERSION}"
VIAddVersionKey "ProductName" "VoiceBar"
VIAddVersionKey "ProductVersion" "${APP_VERSION}"
VIAddVersionKey "FileDescription" "VoiceBar native installer"
VIAddVersionKey "FileVersion" "${FILE_VERSION}"
VIAddVersionKey "LegalCopyright" "Copyright 2026 MLGBJDLW"
BrandingText "VoiceBar"
ShowInstDetails show
ShowUninstDetails show

!define MUI_ABORTWARNING
!define MUI_FINISHPAGE_RUN "$INSTDIR\VoiceBar.exe"
!define MUI_FINISHPAGE_RUN_PARAMETERS "--hidden"
!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_LICENSE "LICENSE"
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "SimpChinese"
!insertmacro MUI_LANGUAGE "English"

Var UpdateMode
Var UpdatePid

Function .onInit
  SetShellVarContext current
  ${IfNot} ${RunningX64}
    MessageBox MB_ICONSTOP "VoiceBar requires Windows x64."
    SetErrorLevel 1
    Abort
  ${EndIf}
  ; Keep the original current-user install path, even in silent update mode.
  StrCpy $INSTDIR "$LOCALAPPDATA\VoiceBar"
  StrCpy $UpdateMode 0
  ${GetParameters} $R0
  ClearErrors
  ${GetOptions} "$R0" "/UPDATE" $R1
  ${IfNot} ${Errors}
    StrCpy $UpdateMode 1
    ClearErrors
    ${GetOptions} "$R0" "/VOICEBAR_PID=" $UpdatePid
    ${If} ${Errors}
      SetErrorLevel 2
      Abort
    ${EndIf}
    ; The UI requests shutdown after spawning us. Wait for its process handle;
    ; never terminate a recording or another process from the installer.
    System::Call 'kernel32::OpenProcess(i 0x100000, i 0, i $UpdatePid) p.r0'
    ${If} $0 != 0
      System::Call 'kernel32::WaitForSingleObject(p r0, i 120000) i.r1'
      System::Call 'kernel32::CloseHandle(p r0)'
      ${If} $1 != 0
        MessageBox MB_ICONSTOP "VoiceBar is still running. Quit it and run this installer again."
        SetErrorLevel 3
        Abort
      ${EndIf}
    ${EndIf}
  ${EndIf}
FunctionEnd

Section "VoiceBar" SEC_MAIN
  SetShellVarContext current
  SetOutPath "$INSTDIR"
  SetOverwrite on
  ClearErrors
  File /oname=VoiceBar.exe "${PAYLOAD_DIR}\VoiceBar.exe"
  !if /FileExists "${PAYLOAD_DIR}\*.dll"
    File "${PAYLOAD_DIR}\*.dll"
  !endif
  File /oname=LICENSE "LICENSE"
  ${If} ${Errors}
    MessageBox MB_ICONSTOP "Could not replace VoiceBar. Quit VoiceBar from its tray menu and retry."
    SetErrorLevel 4
    Abort
  ${EndIf}
  ; This one legacy dependency is obsolete after the native migration.
  Delete "$INSTDIR\WebView2Loader.dll"
  WriteUninstaller "$INSTDIR\uninstall.exe"
  CreateDirectory "$SMPROGRAMS\VoiceBar"
  CreateShortcut "$SMPROGRAMS\VoiceBar\VoiceBar.lnk" "$INSTDIR\VoiceBar.exe"
  CreateShortcut "$SMPROGRAMS\VoiceBar\Uninstall.lnk" "$INSTDIR\uninstall.exe"
  WriteRegStr HKCU "Software\VoiceBar" "InstallDir" "$INSTDIR"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\VoiceBar" "DisplayName" "VoiceBar"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\VoiceBar" "DisplayVersion" "${APP_VERSION}"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\VoiceBar" "Publisher" "MLGBJDLW"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\VoiceBar" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\VoiceBar" "DisplayIcon" "$INSTDIR\VoiceBar.exe"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\VoiceBar" "UninstallString" '$\"$INSTDIR\uninstall.exe$\"'
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\VoiceBar" "QuietUninstallString" '$\"$INSTDIR\uninstall.exe$\" /S'
  WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\VoiceBar" "NoModify" 1
  WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\VoiceBar" "NoRepair" 1
  ${If} $UpdateMode == 1
    Exec '"$INSTDIR\VoiceBar.exe" --hidden'
  ${EndIf}
SectionEnd

Section "Uninstall"
  SetShellVarContext current
  ClearErrors
  Delete "$INSTDIR\VoiceBar.exe"
  ${If} ${Errors}
    MessageBox MB_ICONSTOP "Quit VoiceBar from its tray menu before uninstalling."
    SetErrorLevel 5
    Abort
  ${EndIf}
  ; Known app-local compiler runtimes only; never recursively remove user data.
  Delete "$INSTDIR\libgcc_s_seh-1.dll"
  Delete "$INSTDIR\libstdc++-6.dll"
  Delete "$INSTDIR\libwinpthread-1.dll"
  Delete "$INSTDIR\LICENSE"
  Delete "$INSTDIR\uninstall.exe"
  Delete "$SMPROGRAMS\VoiceBar\VoiceBar.lnk"
  Delete "$SMPROGRAMS\VoiceBar\Uninstall.lnk"
  RMDir "$SMPROGRAMS\VoiceBar"
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "VoiceBar"
  DeleteRegKey HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\VoiceBar"
  DeleteRegKey HKCU "Software\VoiceBar"
  ; Preserve %APPDATA%\studio.voicebar.desktop and any logs or update files.
  RMDir "$INSTDIR"
SectionEnd
