; Tauri NSIS installer hooks (bundle.windows.nsis.installerHooks).
;
; An agent such as Codex or DeepSeek Harness keeps Agent HUD's helpers
; running after the app quits: its `Agent HUD.exe --mcp` relay, the MCP
; helper and the engine. An update then could not write their files
; (Open #146). Before the files are copied, stop those that run from this
; install's folder, then move aside any helper still held (a running
; program can be renamed, not replaced), so the new one is always written.
; The main app is left to the installer's own check that follows, which asks
; before closing it. If the install stops after that, the moved programs are
; put back, so a failed update never leaves the old install without them.

!macro AGENT_HUD_MOVE_ASIDE NAME
  Delete "$INSTDIR\${NAME}.old"
  IfFileExists "$INSTDIR\${NAME}" 0 +2
    Rename "$INSTDIR\${NAME}" "$INSTDIR\${NAME}.old"
!macroend

!macro AGENT_HUD_PUT_BACK NAME
  IfFileExists "$INSTDIR\${NAME}" +3 0
  IfFileExists "$INSTDIR\${NAME}.old" 0 +2
    Rename "$INSTDIR\${NAME}.old" "$INSTDIR\${NAME}"
!macroend

!macro AGENT_HUD_DELETE_MOVED
  Delete "$INSTDIR\agent-hud-engine.exe.old"
  Delete "$INSTDIR\agent-hud-mcp.exe.old"
!macroend

!macro NSIS_HOOK_PREINSTALL
  ; A failed earlier update may have left them moved aside.
  !insertmacro AGENT_HUD_PUT_BACK "agent-hud-engine.exe"
  !insertmacro AGENT_HUD_PUT_BACK "agent-hud-mcp.exe"
  ; Only processes run from this folder: other installs and test builds keep
  ; running. The installer is 32-bit, so processes are matched through WMI,
  ; which gives a 64-bit process's path where Get-Process does not. The
  ; folder goes through the environment, so no character in it can break
  ; the command.
  System::Call 'Kernel32::SetEnvironmentVariable(t "AGENT_HUD_INSTALL_DIR", t "$INSTDIR")'
  nsExec::Exec `"$SYSDIR\WindowsPowerShell\v1.0\powershell.exe" -NoProfile -NonInteractive -ExecutionPolicy Bypass -Command "$$d = [IO.Path]::GetFullPath($$env:AGENT_HUD_INSTALL_DIR).TrimEnd('\') + '\'; Get-CimInstance Win32_Process | Where-Object { $$_.ExecutablePath -and $$_.ExecutablePath.StartsWith($$d, [StringComparison]::OrdinalIgnoreCase) -and ($$_.Name -in 'agent-hud-engine.exe','agent-hud-mcp.exe' -or ($$_.Name -eq 'Agent HUD.exe' -and $$_.CommandLine -match '--mcp')) } | ForEach-Object { Stop-Process -Id $$_.ProcessId -Force -ErrorAction SilentlyContinue }"`
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

; Tauri's template defines no .onInstFailed, so the hooks can.
Function .onInstFailed
  !insertmacro AGENT_HUD_PUT_BACK "agent-hud-engine.exe"
  !insertmacro AGENT_HUD_PUT_BACK "agent-hud-mcp.exe"
FunctionEnd
