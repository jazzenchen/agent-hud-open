; Tauri NSIS installer hooks (bundle.windows.nsis.installerHooks).
;
; An agent such as Codex keeps Agent HUD's engine and MCP helper running
; after the app quits, so an update could not write their files (Open #146).
; Before the files are copied, stop this install's helpers, then move aside
; any that is still held (a running program can be renamed, not replaced),
; so the new one is always written. The moved copies are deleted once
; nothing runs them. `Agent HUD.exe` itself, `--mcp` relays included, is
; left to the installer's own check that follows, which finds what runs it
; by its path: moving it first would hide the running app from that check.

!macro AGENT_HUD_MOVE_ASIDE NAME
  Delete "$INSTDIR\${NAME}.old"
  IfFileExists "$INSTDIR\${NAME}" 0 +2
    Rename "$INSTDIR\${NAME}" "$INSTDIR\${NAME}.old"
!macroend

!macro AGENT_HUD_DELETE_MOVED
  Delete "$INSTDIR\agent-hud-engine.exe.old"
  Delete "$INSTDIR\agent-hud-mcp.exe.old"
!macroend

!macro NSIS_HOOK_PREINSTALL
  ; Only copies in this folder: other installs and test builds keep running.
  ; The folder goes through the environment, so no character in it can
  ; break the command.
  System::Call 'Kernel32::SetEnvironmentVariable(t "AGENT_HUD_INSTALL_DIR", t "$INSTDIR")'
  nsExec::Exec `"$SYSDIR\WindowsPowerShell\v1.0\powershell.exe" -NoProfile -NonInteractive -ExecutionPolicy Bypass -Command "$$d = [IO.Path]::GetFullPath($$env:AGENT_HUD_INSTALL_DIR).TrimEnd('\') + '\'; Get-Process -Name 'agent-hud-engine','agent-hud-mcp' -ErrorAction SilentlyContinue | Where-Object { $$_.Path -and $$_.Path.StartsWith($$d, [StringComparison]::OrdinalIgnoreCase) } | Stop-Process -Force -ErrorAction SilentlyContinue"`
  Pop $0
  Sleep 500
  !insertmacro AGENT_HUD_MOVE_ASIDE "agent-hud-engine.exe"
  !insertmacro AGENT_HUD_MOVE_ASIDE "agent-hud-mcp.exe"
!macroend

!macro NSIS_HOOK_POSTINSTALL
  !insertmacro AGENT_HUD_DELETE_MOVED
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  !insertmacro AGENT_HUD_DELETE_MOVED
  RMDir "$INSTDIR"
!macroend
