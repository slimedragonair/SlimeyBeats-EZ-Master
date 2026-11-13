Unicode true
RequestExecutionLevel user
SetCompressor /SOLID zlib
!include "MUI2.nsh"
!include "FileFunc.nsh"
!include "LogicLib.nsh"
!include "x64.nsh"
!include "WinVer.nsh"

!define COMPANY "SlimeDragon Audio"
!define APPKEY "Software\SlimeDragon Audio\${APP_ID}"
!define UNINSTALLKEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\${APP_ID}"
!define STARTFOLDER "SlimeDragon Audio\${APP_ID}"
Name "${PRODUCT} ${APP_VERSION}"
OutFile "${OUTPUT_FILE}"
InstallDir "$LOCALAPPDATA\Programs\SlimeDragon Audio\${APP_ID}"
InstallDirRegKey HKCU "${APPKEY}" "InstallDir"
BrandingText "SlimeDragon Audio"
Icon "${APP_ICON}"
UninstallIcon "${APP_ICON}"
VIProductVersion "${NUMERIC_VERSION}"
VIAddVersionKey "ProductName" "${PRODUCT}"
VIAddVersionKey "FileDescription" "SlimeyBeats-EZ-Master Setup"
VIAddVersionKey "FileVersion" "${APP_VERSION}"
VIAddVersionKey "ProductVersion" "${APP_VERSION}"
VIAddVersionKey "CompanyName" "${COMPANY}"
VIAddVersionKey "LegalCopyright" "Copyright 2026 SlimeDragon Audio contributors; MIT"
!define MUI_ICON "${APP_ICON}"
!define MUI_UNICON "${APP_ICON}"
!define MUI_ABORTWARNING
!define MUI_WELCOMEPAGE_TITLE "Welcome to ${PRODUCT}"
!define MUI_WELCOMEPAGE_TEXT "Local mastering and real-time EQ.$\r$\n$\r$\nYour music stays yours. This application claims no ownership or royalties in your audio.$\r$\n$\r$\nSetup installs for your current Windows account. WAV processing works immediately; optional compressed formats use a separate FFmpeg installation."
!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_COMPONENTS
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!define MUI_FINISHPAGE_RUN "$INSTDIR\SlimeyBeats-EZ-Master.exe"
!define MUI_FINISHPAGE_RUN_TEXT "Open SlimeyBeats-EZ-Master"
!define MUI_FINISHPAGE_TEXT "Installation is complete.$\r$\n$\r$\nAdd audio, analyze, master, compare and export when ready.$\r$\n$\r$\nAn example track and listening page are available from the Start menu."
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "English"

Function .onInit
  ${IfNot} ${RunningX64}
    MessageBox MB_ICONSTOP "This release requires 64-bit Windows." /SD IDOK
    SetErrorLevel 2
    Abort
  ${EndIf}
  ${IfNot} ${AtLeastWin10}
    MessageBox MB_ICONSTOP "This release requires Windows 10 or newer." /SD IDOK
    SetErrorLevel 2
    Abort
  ${EndIf}
  SetRegView 64
  SetShellVarContext current
  ; NSIS removes /D= from $CMDLINE; read the original command line before upgrade lookup.
  System::Call 'kernel32::GetCommandLine() t .r1'
  ClearErrors
  ${GetOptions} $1 "/D=" $2
  ${If} ${Errors}
    ReadRegStr $0 HKCU "${APPKEY}" "InstallDir"
    StrCmp $0 "" +2
      StrCpy $INSTDIR $0
  ${Else}
    StrCpy $INSTDIR $2
  ${EndIf}
FunctionEnd

Function ValidateInstallDirectory
  ; The trailing separator lets NSIS normalize a directory that does not exist yet.
  ClearErrors
  GetFullPathName $1 "$INSTDIR\"
  IfErrors invalid_directory
  StrCpy $INSTDIR $1
  StrCpy $2 $INSTDIR 1 -1
  ${If} $2 == "\"
    StrLen $2 $INSTDIR
    ${If} $2 > 3
      StrCpy $INSTDIR $INSTDIR -1
    ${EndIf}
  ${EndIf}
  ${GetRoot} "$INSTDIR" $0
  StrCmp $INSTDIR "$0" invalid_directory
  StrCmp $INSTDIR "$0\" invalid_directory
  StrCmp $INSTDIR "$PROFILE" invalid_directory
  StrCmp $INSTDIR "$LOCALAPPDATA" invalid_directory
  StrCmp $INSTDIR "$WINDIR" invalid_directory
  StrCmp $INSTDIR "" invalid_directory
  ; A preview must not upgrade the other channel's installed program folder.
  ReadRegStr $3 HKCU "Software\SlimeDragon Audio\${OTHER_APP_ID}" "InstallDir"
  StrCmp $3 "" check_channel_marker
  GetFullPathName $3 "$3\"
  StrCpy $4 "$INSTDIR\"
  StrCmp $3 $4 invalid_directory
check_channel_marker:
  ReadINIStr $3 "$INSTDIR\install-info.ini" "Install" "Channel"
  StrCmp $3 "" valid_directory
  StrCmp $3 "${APP_CHANNEL}" valid_directory invalid_directory
valid_directory:
  Return
invalid_directory:
  MessageBox MB_ICONSTOP "Choose a dedicated application folder." /SD IDOK
  SetErrorLevel 2
  Abort
FunctionEnd

Function .onVerifyInstDir
  Call ValidateInstallDirectory
FunctionEnd

Section "Application, guides and example audio" MainSection
  SectionIn RO
  Call ValidateInstallDirectory
  SetShellVarContext current
  SetRegView 64
  SetOverwrite on
  !include "${INSTALL_FILES}"
  SetOutPath "$INSTDIR"
  WriteUninstaller "$INSTDIR\Uninstall.exe"
  WriteINIStr "$INSTDIR\install-info.ini" "Install" "Version" "${APP_VERSION}"
  WriteINIStr "$INSTDIR\install-info.ini" "Install" "Channel" "${APP_CHANNEL}"
  ReadINIStr $0 "$INSTDIR\install-info.ini" "Shortcuts" "Desktop"
  StrCmp $0 "1" +2
    WriteINIStr "$INSTDIR\install-info.ini" "Shortcuts" "Desktop" "0"
  CreateDirectory "$SMPROGRAMS\${STARTFOLDER}"
  CreateShortcut "$SMPROGRAMS\${STARTFOLDER}\${PRODUCT}.lnk" "$INSTDIR\SlimeyBeats-EZ-Master.exe" "" "$INSTDIR\SlimeyBeats.ico"
  CreateShortcut "$SMPROGRAMS\${STARTFOLDER}\Example audio.lnk" "$INSTDIR\Examples\Listen.html"
  CreateShortcut "$SMPROGRAMS\${STARTFOLDER}\User guide.lnk" "$INSTDIR\docs\GETTING_STARTED.html"
  CreateShortcut "$SMPROGRAMS\${STARTFOLDER}\Connect AI assistant.lnk" "$INSTDIR\Connect AI Assistant.bat"
  CreateShortcut "$SMPROGRAMS\${STARTFOLDER}\Uninstall.lnk" "$INSTDIR\Uninstall.exe"
  WriteRegStr HKCU "${APPKEY}" "InstallDir" "$INSTDIR"
  WriteRegStr HKCU "${UNINSTALLKEY}" "DisplayName" "${PRODUCT}"
  WriteRegStr HKCU "${UNINSTALLKEY}" "DisplayVersion" "${APP_VERSION}"
  WriteRegStr HKCU "${UNINSTALLKEY}" "Publisher" "${COMPANY}"
  WriteRegStr HKCU "${UNINSTALLKEY}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "${UNINSTALLKEY}" "DisplayIcon" "$INSTDIR\SlimeyBeats.ico"
  WriteRegStr HKCU "${UNINSTALLKEY}" "UninstallString" '$\"$INSTDIR\Uninstall.exe$\"'
  WriteRegStr HKCU "${UNINSTALLKEY}" "QuietUninstallString" '$\"$INSTDIR\Uninstall.exe$\" /S'
  WriteRegDWORD HKCU "${UNINSTALLKEY}" "EstimatedSize" ${ESTIMATED_KIB}
  WriteRegDWORD HKCU "${UNINSTALLKEY}" "NoModify" 1
  WriteRegDWORD HKCU "${UNINSTALLKEY}" "NoRepair" 1
SectionEnd

Section /o "Desktop shortcut" DesktopSection
  CreateShortcut "$DESKTOP\${PRODUCT}.lnk" "$INSTDIR\SlimeyBeats-EZ-Master.exe" "" "$INSTDIR\SlimeyBeats.ico"
  WriteINIStr "$INSTDIR\install-info.ini" "Shortcuts" "Desktop" "1"
SectionEnd

!insertmacro MUI_FUNCTION_DESCRIPTION_BEGIN
  !insertmacro MUI_DESCRIPTION_TEXT ${MainSection} "The native mastering app, batch CLI, licenses, offline guides and a CC0 example track."
  !insertmacro MUI_DESCRIPTION_TEXT ${DesktopSection} "Add a shortcut for your current account."
!insertmacro MUI_FUNCTION_DESCRIPTION_END

Section "Uninstall"
  SetShellVarContext current
  SetRegView 64
  ReadINIStr $0 "$INSTDIR\install-info.ini" "Shortcuts" "Desktop"
  StrCmp $0 "1" 0 +2
    Delete "$DESKTOP\${PRODUCT}.lnk"
  Delete "$SMPROGRAMS\${STARTFOLDER}\${PRODUCT}.lnk"
  Delete "$SMPROGRAMS\${STARTFOLDER}\Example audio.lnk"
  Delete "$SMPROGRAMS\${STARTFOLDER}\User guide.lnk"
  Delete "$SMPROGRAMS\${STARTFOLDER}\Connect AI assistant.lnk"
  Delete "$SMPROGRAMS\${STARTFOLDER}\Uninstall.lnk"
  RMDir "$SMPROGRAMS\${STARTFOLDER}"
  RMDir "$SMPROGRAMS\SlimeDragon Audio"
  !include "${UNINSTALL_FILES}"
  Delete "$INSTDIR\install-info.ini"
  Delete "$INSTDIR\Uninstall.exe"
  DeleteRegKey HKCU "${UNINSTALLKEY}"
  DeleteRegKey HKCU "${APPKEY}"
  DeleteRegKey /ifempty HKCU "Software\SlimeDragon Audio"
  ; Only empty folders are removed; user settings, exports, profiles and Examples remain.
  RMDir "$INSTDIR"
SectionEnd
