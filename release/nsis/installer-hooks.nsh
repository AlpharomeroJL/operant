; Operant NSIS installer hooks.
;
; Wired in from ui/src-tauri/tauri.conf.json via bundle.windows.nsis.installerHooks
; (relative path "../../release/nsis/installer-hooks.nsh" from that file). Tauri's
; NSIS template does this at include time:
;   {{#if installer_hooks}} !include "{{installer_hooks}}" {{/if}}
; with the path resolved relative to the tauri.conf.json directory, same as every
; other path field in bundle.windows.nsis (icon, headerImage, template, ...).
;
; STATUS: written against the public NSIS_HOOK_* macro contract documented for
; Tauri v2 (installer_hooks field on NsisConfig). NSIS and cargo-tauri are not
; installed in this environment (see release/REPRODUCIBLE.md), so this file has
; not been compiled or run by a real installer build. Treat it as a reviewed
; starting point, not a verified artifact: have someone confirm it compiles and
; behaves as expected on the first real `cargo tauri build` on Windows, before
; it ships in a release. See release/KEYS.md for the same caveat applied to the
; updater signing format.
;
; Covers the two things docs/specs/release.md asks for that the base Tauri NSIS
; template does not do on its own:
;   1. Add the install directory to PATH on install, remove it on uninstall, so
;      the operant CLI works from any terminal after a default (per-user) install.
;   2. Prompt before deleting user data (workflows, recordings) on uninstall;
;      the base template only ever removes program binaries and registry keys,
;      never $APPDATA content.
;
; Uses only core NSIS instructions plus the stock WinMessages.nsh header (no
; third-party plugin such as EnVar), since this environment cannot verify which
; plugins ship with Tauri's bundled makensis.
;
; NSIS requires installer-time and uninstaller-time code to be separate
; functions (the "un." prefix marks a function as compiled into the embedded
; uninstaller). AddInstDirToPath/StrContains run during install;
; un.RemoveInstDirFromPath/un.StrContains/un.StrReplace run during uninstall.
; They are written out twice rather than sharing a macro, on purpose: fully
; spelled out is easier to hand-verify than a macro expansion, given nothing
; here can be compiled in this environment to catch a mistake.

!include "WinMessages.nsh"

!macro NSIS_HOOK_POSTINSTALL
  Push $0
  Push $1
  Push $2
  Call AddInstDirToPath
  Pop $2
  Pop $1
  Pop $0
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  MessageBox MB_YESNO|MB_ICONQUESTION \
    "Remove saved Operant workflows and recordings too?$\r$\n$\r$\nChoose No to keep them on disk after uninstalling." \
    IDYES operant_remove_user_data
  Goto operant_keep_user_data
  operant_remove_user_data:
    RMDir /r "$APPDATA\Operant"
  operant_keep_user_data:
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  Push $0
  Push $1
  Push $2
  Call un.RemoveInstDirFromPath
  Pop $2
  Pop $1
  Pop $0
!macroend

; ---------------------------------------------------------------------------
; Install-time helpers
; ---------------------------------------------------------------------------

Function AddInstDirToPath
  ; Per-user PATH lives at HKCU\Environment. Safe even under a per-machine
  ; (installMode "both", administrator-elevated) install: it only ever
  ; extends the current user's own PATH, never HKLM's.
  ReadRegStr $0 HKCU "Environment" "Path"
  StrCmp $0 "" operant_i_path_set
    Push $0
    Push "$INSTDIR"
    Call StrContains
    Pop $1
    StrCmp $1 "1" operant_i_path_done
    StrCpy $0 "$0;$INSTDIR"
    Goto operant_i_path_write
  operant_i_path_set:
    StrCpy $0 "$INSTDIR"
  operant_i_path_write:
    WriteRegExpandStr HKCU "Environment" "Path" "$0"
    SendMessage ${HWND_BROADCAST} ${WM_WININICHANGE} 0 "STR:Environment" /TIMEOUT=5000
  operant_i_path_done:
FunctionEnd

; Returns "1" on the stack if the needle (top of stack) is found in the
; haystack (second on stack), else "".
Function StrContains
  Exch $R0 ; needle
  Exch
  Exch $R1 ; haystack
  Push $R2
  Push $R3
  Push $R4
  Push $R5

  StrCpy $R2 -1
  StrLen $R3 $R0
  StrLen $R4 $R1

  operant_i_contains_loop:
    IntOp $R2 $R2 + 1
    IntOp $R5 $R2 + $R3
    IntCmp $R5 $R4 0 0 operant_i_contains_notfound
    StrCpy $R5 $R1 $R3 $R2
    StrCmp $R5 $R0 operant_i_contains_found operant_i_contains_loop

  operant_i_contains_notfound:
    StrCpy $R0 ""
    Goto operant_i_contains_done
  operant_i_contains_found:
    StrCpy $R0 "1"

  operant_i_contains_done:
    Pop $R5
    Pop $R4
    Pop $R3
    Pop $R2
    Pop $R1
    Exch $R0
FunctionEnd

; ---------------------------------------------------------------------------
; Uninstall-time helpers
; ---------------------------------------------------------------------------

Function un.RemoveInstDirFromPath
  ReadRegStr $0 HKCU "Environment" "Path"
  StrCmp $0 "" operant_u_rm_done
    Push $0
    Push "$INSTDIR"
    Call un.StrContains
    Pop $1
    StrCmp $1 "1" 0 operant_u_rm_done
    ; Strip whichever exact form of the segment is present: ";$INSTDIR",
    ; "$INSTDIR;", or a bare "$INSTDIR" (the only entry in PATH).
    StrCpy $2 "$0;"
    Push $2
    Push "$INSTDIR;"
    Call un.StrReplace
    Pop $2
    Push $2
    Push ";$INSTDIR"
    Call un.StrReplace
    Pop $2
    Push $2
    Push "$INSTDIR"
    Call un.StrReplace
    Pop $2
    StrCpy $0 $2 -1 ; drop the trailing ";" appended above
    WriteRegExpandStr HKCU "Environment" "Path" "$0"
    SendMessage ${HWND_BROADCAST} ${WM_WININICHANGE} 0 "STR:Environment" /TIMEOUT=5000
  operant_u_rm_done:
FunctionEnd

; Returns "1" on the stack if the needle (top of stack) is found in the
; haystack (second on stack), else "".
Function un.StrContains
  Exch $R0 ; needle
  Exch
  Exch $R1 ; haystack
  Push $R2
  Push $R3
  Push $R4
  Push $R5

  StrCpy $R2 -1
  StrLen $R3 $R0
  StrLen $R4 $R1

  operant_u_contains_loop:
    IntOp $R2 $R2 + 1
    IntOp $R5 $R2 + $R3
    IntCmp $R5 $R4 0 0 operant_u_contains_notfound
    StrCpy $R5 $R1 $R3 $R2
    StrCmp $R5 $R0 operant_u_contains_found operant_u_contains_loop

  operant_u_contains_notfound:
    StrCpy $R0 ""
    Goto operant_u_contains_done
  operant_u_contains_found:
    StrCpy $R0 "1"

  operant_u_contains_done:
    Pop $R5
    Pop $R4
    Pop $R3
    Pop $R2
    Pop $R1
    Exch $R0
FunctionEnd

; Removes the first occurrence of needle (top of stack) from haystack (second
; on stack) and returns the result on the stack. Only ever called after
; un.StrContains has already confirmed the needle is present.
Function un.StrReplace
  Exch $R0 ; needle
  Exch
  Exch $R1 ; haystack
  Push $R2
  Push $R3
  Push $R4
  Push $R5
  Push $R6

  StrCpy $R2 -1
  StrLen $R3 $R0
  StrLen $R4 $R1

  operant_u_replace_loop:
    IntOp $R2 $R2 + 1
    IntOp $R5 $R2 + $R3
    IntCmp $R5 $R4 0 0 operant_u_replace_notfound
    StrCpy $R5 $R1 $R3 $R2
    StrCmp $R5 $R0 operant_u_replace_found operant_u_replace_loop

  operant_u_replace_notfound:
    StrCpy $R6 $R1
    Goto operant_u_replace_done
  operant_u_replace_found:
    StrCpy $R5 $R1 $R2
    IntOp $R2 $R2 + $R3
    StrCpy $R6 $R1 "" $R2
    StrCpy $R6 "$R5$R6"

  operant_u_replace_done:
    Pop $R5
    Pop $R4
    Pop $R3
    Pop $R2
    Pop $R1
    Pop $R0
    Push $R6
FunctionEnd
