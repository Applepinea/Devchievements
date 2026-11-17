; installer-hooks.nsh - prerequisite checks that run BEFORE any application file
; is copied (tauri's template calls NSIS_HOOK_PREINSTALL inside Section Install,
; right after SetOutPath and before the File commands).
;
; Why here rather than in the app: the bundled WebView2 section decides by reading
; the registry, and a Windows image can carry the EdgeUpdate key with no runtime on
; disk (measured on a VM, 2026-11-17: the installer skipped WebView2, the app then
; painted an empty window and died inside wry). A check that only trusts the
; registry installs a program that cannot draw.
;
; Two rules this file follows:
;   1. verify the file, not the registration;
;   2. fail before copying anything, so there is nothing to roll back.

!include "LogicLib.nsh"

!define DEVCH_WEBVIEW2_GUID "{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}"
!define DEVCH_WEBVIEW2_URL "https://developer.microsoft.com/microsoft-edge/webview2/"
!define DEVCH_GIT_URL "https://git-scm.com/download/win"

Var DevchWebViewVersion
Var DevchWebViewOk
Var DevchGitOk
Var DevchBootstrapResult

; Is msedgewebview2.exe actually on disk for the registered version?
Function DevchCheckWebView2
  StrCpy $DevchWebViewOk 0
  StrCpy $DevchWebViewVersion ""

  ReadRegStr $DevchWebViewVersion HKLM "SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\${DEVCH_WEBVIEW2_GUID}" "pv"
  ${If} $DevchWebViewVersion == ""
    ReadRegStr $DevchWebViewVersion HKLM "SOFTWARE\Microsoft\EdgeUpdate\Clients\${DEVCH_WEBVIEW2_GUID}" "pv"
  ${EndIf}
  ${If} $DevchWebViewVersion == ""
    ReadRegStr $DevchWebViewVersion HKCU "SOFTWARE\Microsoft\EdgeUpdate\Clients\${DEVCH_WEBVIEW2_GUID}" "pv"
  ${EndIf}

  ${If} $DevchWebViewVersion != ""
    ${If} ${FileExists} "$PROGRAMFILES64\Microsoft\EdgeWebView\Application\$DevchWebViewVersion\msedgewebview2.exe"
      StrCpy $DevchWebViewOk 1
    ${ElseIf} ${FileExists} "$PROGRAMFILES\Microsoft\EdgeWebView\Application\$DevchWebViewVersion\msedgewebview2.exe"
      StrCpy $DevchWebViewOk 1
    ${ElseIf} ${FileExists} "$LOCALAPPDATA\Microsoft\EdgeWebView\Application\$DevchWebViewVersion\msedgewebview2.exe"
      StrCpy $DevchWebViewOk 1
    ${Else}
      DetailPrint "WebView2 is registered as $DevchWebViewVersion but its files are missing."
    ${EndIf}
  ${EndIf}
FunctionEnd

; Install the runtime with the bootstrapper shipped next to this script.
Function DevchInstallWebView2
  DetailPrint "Installing the Microsoft Edge WebView2 Runtime..."
  SetOutPath "$TEMP"
  File "/oname=DevchWebView2Setup.exe" "${__FILEDIR__}\..\assets\webview2\MicrosoftEdgeWebView2Setup.exe"
  ExecWait '"$TEMP\DevchWebView2Setup.exe" /silent /install' $DevchBootstrapResult
  Delete "$TEMP\DevchWebView2Setup.exe"
  DetailPrint "WebView2 bootstrapper exit code: $DevchBootstrapResult"
FunctionEnd

; Git is a warning, not a gate: without it the app still starts and explains itself,
; it just has nothing to read.
Function DevchCheckGit
  StrCpy $DevchGitOk 0
  SearchPath $0 "git.exe"
  ${If} $0 != ""
    StrCpy $DevchGitOk 1
  ${ElseIf} ${FileExists} "$PROGRAMFILES\Git\cmd\git.exe"
    StrCpy $DevchGitOk 1
  ${ElseIf} ${FileExists} "$PROGRAMFILES64\Git\cmd\git.exe"
    StrCpy $DevchGitOk 1
  ${ElseIf} ${FileExists} "$LOCALAPPDATA\Programs\Git\cmd\git.exe"
    StrCpy $DevchGitOk 1
  ${EndIf}
FunctionEnd

!macro NSIS_HOOK_PREINSTALL
  ; Test hook: force the abort path without uninstalling a runtime from the build
  ; machine. It skips the repair step too, so the check cannot quietly pass.
  ReadEnvStr $0 "DEVCH_FORCE_NO_WEBVIEW2"
  ${If} $0 == "1"
    StrCpy $DevchWebViewOk 0
    StrCpy $DevchWebViewVersion "forced-missing"
  ${Else}
    Call DevchCheckWebView2
    ${If} $DevchWebViewOk = 0
      Call DevchInstallWebView2
      Call DevchCheckWebView2
    ${EndIf}
  ${EndIf}

  ${If} $DevchWebViewOk = 0
    ; Nothing has been copied yet, so failing here leaves no broken installation.
    ; `Quit` rather than `Abort`: a silent run must terminate immediately and with a
    ; code, and an abort inside a section has been observed to leave the process
    ; waiting rather than exiting.
    ${IfNot} ${Silent}
      MessageBox MB_ICONSTOP|MB_OK "Devchievements needs the Microsoft Edge WebView2 Runtime, and it could not be installed automatically.$\r$\n$\r$\nInstall it from:$\r$\n${DEVCH_WEBVIEW2_URL}$\r$\n$\r$\nThen run this installer again. Nothing has been changed on this machine."
      ExecShell "open" "${DEVCH_WEBVIEW2_URL}"
    ${EndIf}
    SetErrorLevel 2
    Quit
  ${EndIf}

  Call DevchCheckGit
  ${If} $DevchGitOk = 0
    ${IfNot} ${Silent}
      MessageBox MB_ICONEXCLAMATION|MB_OK "Git for Windows was not found.$\r$\n$\r$\nDevchievements can be installed and started, but it cannot record commits without Git. Install it from:$\r$\n${DEVCH_GIT_URL}$\r$\n"
    ${EndIf}
    DetailPrint "Git for Windows not found; the app will run without commit capture."
  ${EndIf}
!macroend
