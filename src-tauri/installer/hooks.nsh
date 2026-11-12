; ============================================================
; hooks.nsh
; Extra steps for the Windows installer and uninstaller.
; ============================================================

; Blip 1.x (the AutoHotkey one) lived in %LOCALAPPDATA%\Programs\Blip. Before
; the new Blip goes in, close the old one so there aren't two blips per click,
; and remove it with its own uninstaller.
!macro NSIS_HOOK_PREINSTALL
  Push $0
  Push $1
  SetShellVarContext current
  ${If} ${FileExists} "$LOCALAPPDATA\Programs\Blip\Blip.exe"
    ; Only the old copy: the new Blip is also called Blip.exe.
    nsExec::Exec `powershell -NoProfile -NonInteractive -Command "Get-Process -Name Blip -ErrorAction SilentlyContinue | Where-Object { $$_.Path -like '*\Programs\Blip\*' } | Stop-Process -Force"`
    Pop $0
    ${If} ${FileExists} "$LOCALAPPDATA\Programs\Blip\unins000.exe"
      ExecWait '"$LOCALAPPDATA\Programs\Blip\unins000.exe" /VERYSILENT /SUPPRESSMSGBOXES /NORESTART'
      ; It hands over to a copy of itself and returns at once: wait (up to
      ; 10 seconds) until the old files are really gone.
      StrCpy $1 0
      ${DoWhile} ${FileExists} "$LOCALAPPDATA\Programs\Blip\unins000.exe"
        ${If} $1 >= 50
          ${ExitDo}
        ${EndIf}
        Sleep 200
        IntOp $1 $1 + 1
      ${Loop}
    ${EndIf}
  ${EndIf}
  Pop $1
  Pop $0
!macroend

; Tauri's "delete data" checkbox only deletes folders named after the app's ID,
; but Blip keeps its things in "Blip" folders. So when the box is ticked,
; also delete those: the settings (Roaming) and the browser data (Local).
; Never while updating.
;
; Blip also introduces itself to Windows so its notifications show
; (notifications.rs). Remove that on a real uninstall, ticked or not.
!macro NSIS_HOOK_POSTUNINSTALL
  ${If} $UpdateMode <> 1
    DeleteRegKey HKCU "Software\Classes\AppUserModelId\${BUNDLEID}"
  ${EndIf}

  ${If} $DeleteAppDataCheckboxState = 1
  ${AndIf} $UpdateMode <> 1
    SetShellVarContext current
    RmDir /r "$APPDATA\${PRODUCTNAME}"
    RmDir /r "$LOCALAPPDATA\${PRODUCTNAME}"
  ${EndIf}
!macroend
