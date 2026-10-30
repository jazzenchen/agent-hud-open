# Client hooks and requests

The public Swift implementation (`Sources/AgentHUDCore/Hooks` in `agent-hud-native-open`) remains the behavioral reference. This port separates pure client formats, shared request ownership, OS transport, and UI. It does not install itself from a preview bundle.

## Ownership

| Layer | Location | Responsibility |
| --- | --- | --- |
| Client formats | `src/crates/agents` | Per-client event/configuration parsing and response encoding. No filesystem, process, socket, credential, or desktop send operation. |
| Shared requests | `src/crates/core/src/requests.rs` | One ephemeral queue, connection cancellation, changed timeouts, dismissal until native settlement, exact Codex turn reviewer evidence, and native response receipts. |
| Session presentation | `src/crates/core/src/hook_events.rs`, `presentation.rs` | Overlay source-reported attention, origins, compaction, completion, and pending requests on fresh session views. Native waiting flags affect phase without fabricating an actionable card or activity clock. No token ledger mutation. |
| Platform IO | `src/crates/platform/src/hooks` | Unix callbacks, pending-only Claude transcript withdrawal, origin ancestry, offline event inbox, reviewed configuration changes, and Codex Desktop approvals/questions. |
| Application | `src/crates/runtime`, desktop `Service` | One runtime, immediate request notifications, existing transcript collection cadence, and independent transport stop/configuration outside the collector lock. |
| UI | `src/packages/hud/src/RequestActions.tsx` | Permission decisions, exact question keys/choices/custom text and skipped questions, plan approval/accept edits/feedback, leave-to-client, and errors. shadcn source is unchanged. |

HUD text fields acquire temporary native keyboard focus only on an explicit text-control click, and release it on withdrawal, blur or unmount. On macOS the HUD is a nonactivating `NSPanel` (see `docs/tao-panel-patch.md`): the click makes it key without activating this app, so the frontmost application stays active, and releasing the lease hands keys back to that application. Other platforms focus a normal Tauri window. This keyboard handoff has not been verified in a live GUI.

The UI's acknowledgment means the local request owner accepted the decision. Native response receipts distinguish confirmed, failed, and uncertain delivery. A missing native acknowledgment never causes an automatic resend. Receipts are bounded and ephemeral; request bodies are not stored as history.

## Current adapters

- Permission callback formats: Claude Code, Codex, Qoder, Qoder CN, QoderWork, CodeBuddy, WorkBuddy, ZCode, Qwen Code, and Devin. Each retains its own capabilities; an unsupported question/plan stays in the original client.
- Completion callback formats: Antigravity, Cursor, Copilot, CodeBuddy, Qwen Code, and Devin.
- Claude attention, automatic compaction, and verified Claude/Codex session origin. An origin never establishes a running turn; attention never reopens a terminal turn. Compaction expires after five minutes or a newer source record.
- Claude Code session endpoint: the origin hook records the host of `ANTHROPIC_BASE_URL` (no path, query or key) and which cloud platform switch is on, as the session's environment holds them, so its calls are recorded under their route (a relay, a cloud platform, a local server) and priced accordingly. At `SessionStart` a host on this computer or its local network is asked over HTTP what serves it (Ollama, LM Studio, llama.cpp, vLLM) and with which context window; nothing else is contacted. The endpoint is kept per session in `ingestion.sqlite` and never synced.
- Codex permissions require the exact turn to explicitly have `approvals_reviewer: "user"`. Unknown, absent, or automatic reviewers remain with the client. A permission policy is not proof of a human reviewer.
- Codex Desktop app/browser approvals and asynchronous questions use the existing owner's local stream. This observer does not create a second conversation engine.

These are adapter implementations, not evidence that every client has been installed or exercised with a real account. Completion-only sessions are labeled `usageStatus: "unavailable"`; zero is a required wire sentinel, not a measured token total. Numeric statistics continue to use the canonical transcript ledger.

## Activation and platform status

The hook owner starts disabled. `connection.clientHooks` distinguishes supported transport, actual listener state, requested setting, installation eligibility, and installation state. Listening alone does not prove that client settings point at this app.

Configuration uses explicit `install_plan` / `apply_plan` with a host-delegated canonical executable. It preserves third-party hook entries, permissions and symlinks, rejects a stale plan, and refuses transient `target`/disk-image/translocated executables. A build of this repository alone has no delegated canonical hook executable, so Settings explains that installation is unavailable. No existing client configuration or original socket is acquired automatically. The old explicit `--install-completion-hook` and `--install-pi-observer` entry points report an unsupported-installation error instead of launching the UI or claiming success; the Pi observer extension itself has not been ported.

The inbox initially reconciles its history, then reads changed JSON files from filesystem notifications; directory/overflow notifications retain a full reconciliation fallback. A fixture with 2,000 records read one record for a single-file update, with no full scan.

Unix transport and isolated protocol fixtures are implemented. Windows requires a named-pipe callback/desktop adapter and remains unsupported; this is reported explicitly rather than emulated with macOS paths. Linux real-client qualification remains outstanding.

## Tests

Fixtures exercise client format and configuration vectors; isolated Unix callback round trips; connection withdrawal; changed hold deadlines; owner restart; pending Claude tool-result withdrawal; third-party configuration preservation, idempotence, symlinks and stale-plan refusal; Codex Desktop stream, patch and response receipts; and request projection without source scanning. A runtime fixture checks that request projection never consumes a usage record or wakes data collection, and that the collector advances the same ledger afterwards. Real client installation and a GUI hook-owner handoff are not covered by tests.
