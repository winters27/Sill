; What the uninstaller takes with it.
;
; Tauri's NSIS template removes the install directory and the shortcuts and
; nothing else, so without this an uninstalled Sill leaves behind the registry
; entry that starts it at sign-in and every megabyte it ever wrote. The first
; is the one somebody notices: Windows keeps trying to start a program that is
; gone, and the only sign is a slower sign-in.
;
; Every path here is also named in `src-tauri/src/leavings.rs`, and
; `verify:source` refuses a build where the two disagree. This file and that
; list are one fact written twice, which is the shape that has gone stale four
; times in this codebase; the rule is what stops it going stale a fifth.

!macro NSIS_HOOK_PREUNINSTALL
  ; Whether the person wants to keep their settings. Asked before anything is
  ; removed, so a No leaves the machine as it was rather than half cleaned.
  Var /GLOBAL SillKeepData
  StrCpy $SillKeepData "1"

  ; A silent uninstall keeps everything. It is run by deployment tooling and
  ; by an upgrade, and neither of those is somebody deciding to throw their
  ; snippets away.
  ${IfNot} ${Silent}
    MessageBox MB_YESNO|MB_ICONQUESTION \
      "Also delete your Sill settings, clipboard history, snippets, quicklinks and installed extensions?$\n$\nChoose No to keep them for when you install Sill again." \
      /SD IDNO IDYES sill_take_data IDNO sill_keep_data
    sill_take_data:
      StrCpy $SillKeepData "0"
      Goto sill_asked
    sill_keep_data:
      StrCpy $SillKeepData "1"
    sill_asked:
  ${EndIf}
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  ; The entry that starts Sill when you sign in.
  ;
  ; Removed always, and never subject to the question above: it is not the
  ; person's data, it is a pointer to a program that no longer exists.
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "Sill"

  ; The automations and reminders Sill asked Windows to run.
  ;
  ; Not a file and not a value Sill wrote for itself: this is work the Task
  ; Scheduler is holding on Sill's behalf, and it outlives the process, the
  ; reboot and the uninstall. A one-off timer deletes itself once it fires,
  ; but a pending one and every daily trigger survive, and what they start is
  ; sill.exe, which by then is gone. Kept, they would fail at three in the
  ; morning forever, in a log nobody reads.
  ;
  ; schtasks rather than deleting a folder: a task lives in the registry as
  ; well as on disk, so removing only the files leaves the service listing
  ; entries it can no longer run. /F so it does not stop to ask, and the
  ; result is discarded because a machine that never had a task is not an
  ; error worth showing somebody who is uninstalling.
  nsExec::ExecToLog '"$SYSDIR\schtasks.exe" /Delete /TN "Task Scheduler\Sill" /F'
  Pop $0


  ; The browser engine's cache for Sill's own windows. Nothing of theirs is in
  ; it that is not also in the folder below, so it goes without asking.
  RMDir /r "$LOCALAPPDATA\app.winters.sill"

  ${If} $SillKeepData == "0"
    RMDir /r "$APPDATA\app.winters.sill"
  ${EndIf}
!macroend
