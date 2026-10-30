# Every client Agent HUD knows

`agent-hud-agents` holds what Agent HUD knows about each client, one folder per client, or per family that shares a format ([contract](../../../docs/agent-contract.md)):

- `src/<client>/mod.rs`: the client's descriptor (`Agent`); `ALL` lists them all in Settings order.
- `src/<client>/transcript.rs`: its transcript parser, which fills in the shared model in `src/transcript/` (`Log`, `LiveLog`, `Usage`, meter events, database records).
- `src/<client>/hooks.rs`: its hooks as data: its settings file, layout and hooks; its permission rules, questions and plans; its completion payload; and how a hook is launched on each OS.
- `src/<client>/account.rs`: its account reader, `read(&dyn Host) -> Result<Reading, String>`. Platform's account scheduler calls them one at a time, in Open's order.
- `src/coding_plans/`: Kimi Code, the GLM Coding Plan and OpenCode Go, read with the credentials other clients already hold (`discover.rs`) from each plan's own usage endpoint (`parse.rs`). One plan can serve several clients, so the readers are shared rather than per client.
- `src/account.rs`: the `Host` an account reader reads through (the OS, home and application data folders, environment, bounded files, file details, folder listings, read-only SQLite queries, Agent HUD's own files, an installed client's program and a JSON-lines engine started from it, the system credential store, saved credentials, consents, JSON over HTTP GET or POST, a client's native channel), its answers (`Found`, `Missing`, `Unsupported`, `Denied`, `Failed`) and the shared `Reading`. Platform implements `Host` per OS; tests use `account::fake::FakeHost`, so every OS's behaviour runs on every runner.
- `src/hooks/`: the hook formats below, which read that data and name no client.
- `src/process.rs`: a hook caller as platform reads it (`Caller`), and where a session started (`Origin`); each client's rules are in its `process.rs`, named by `Hooks::origin` and `Permission::unattended`.
- `src/replies.rs`: how words reach a client's sessions (`Agent::replies`), for the questions core answers through the host.
- `src/channel.rs`: a client's native channel (`Agent::channel`), such as Codex's desktop IPC: what it carries and on which OS it runs. Platform runs it in `platform/src/clients/<id>/`.
- `tests/golden.rs`: every hook format's output for 1,617 inputs, pinned in `tests/golden/hook_formats.json`. A change that alters one must regenerate the fixture (`UPDATE_GOLDEN=1`) and show the diff.

No file in `src/` names an operating system: what differs by OS is data (`PerOs`) or an answer from the `Host`. The crate reads only the logs and databases the engine names; it opens no sockets, starts no processes and reaches no files, network or credentials except through the `Host`.

## Hook formats

`agent-hud-agents` owns public client payloads and JSON configuration transformations. It does not depend on Core, the platform crate, Swift, Tauri, sockets, processes, filesystem paths resolved on the host, credentials, or native desktop message transports. The platform owns installation, safe file I/O, process verification and callback connections. Core owns pending-request lifetimes, expiration, withdrawal, reviewer facts, and event presentation.

## Request API

- `parse_request(source, &payload) -> Result<Option<Request>, String>` normalizes the client source and session identity. Malformed/unsupported questions and tools the client cannot answer here return `None`; unknown clients and invalid top-level payloads return a fixed error.
- `Request` is cloneable and serde round-trippable. `source`, `session_id`, `kind`, `turn_id`, and `requires_user_reviewer` are public. Internal suggestions and original question/plan input survive the round trip, but no request is persisted by this crate.
- `Request::view(id, created_at_ms)` returns the public Approval fields plus questions, original plan, edit preview, suggestions and supported answer capabilities. A question has `id`, `answerKey`, `question`, `header`, `options`, and `multiSelect`.
- `Request::response(&decision)` returns `None` for no opinion: write **no bytes**, then close the callback. Decisions are `allow`, `deny`, `always`, `answer`, `approve_plan`, `keep_planning`, and `leave`. Answers are `{[answerKey]: {selected: string[], custom?: string|null}}`; hook replies map them back to exact question text and join choices/custom words with `", "`.
- Only Claude and the Qoder builds can apply an advertised permanent rule. An `always` suggestion must be an actual advertised, readable `addRules`/`allow` entry. Only Claude can approve plans, optionally entering the session's `acceptEdits` mode. Devin's allow/deny reply uses its top-level `decision` shape.

**Codex must pass the coordinator's external reviewer gate before registration.** A permission-hook payload's policy is not proof that the user reviews the turn. Original source checks the exact turn's desktop state, remembered context or latest matching transcript `turn_context` for reviewer `user`; unknown/automatic review must be released unanswered. This crate sets `requires_user_reviewer = true`; it cannot inspect host state. Desktop-only requests require their separate observer/native responder and are not invented here.

Permission hooks: Claude, Codex, Qoder/CN/Work, CodeBuddy, WorkBuddy, ZCode, Qwen and Devin. Antigravity and DeepSeek use native observers, not permission hooks. Cursor and Copilot have no permission-hook adapter in the original source. Legacy `qoderCN`/`qoderWork` source names normalize to this crate's catalog IDs.

## Configuration API

`configuration(source, full_flag)` returns the default HOME-relative JSON path and event names. Host environment overrides, installation detection, symbolic-link handling, permissions and atomic file replacement remain platform responsibilities.

`patch_configuration(source, full_flag, &current, command)` is a pure JSON transformation; `None` removes owned handlers. `hook_commands` reports owned handler presence, not whether the host is running them. The patch preserves unrelated configuration, foreign handlers, matchers, and custom owned-handler fields. Duplicate owned handlers collapse to the first. Unknown root/event layouts are rejected. Cursor/Copilot support only their known version-1 layout; ZCode uses nested `hooks.events` and preserves an explicit disabled setting. Antigravity's owned `agent-hud` entry preserves its explicit `enabled: false`.

The reporting hooks are asynchronous. Claude/Codex origins patch only `SessionStart` and `UserPromptSubmit`; compaction is patched independently under `PreCompact`. Legacy Claude compaction commands used `--session-origin-hook claude`; the configuration adapter recognizes those only inside `PreCompact`. New hosts may use `--compaction-hook claude` explicitly. Existing commands are recognized by their exact final flag/source suffix, as in the original `HookCommand`; executable/script paths are quoted by the platform, fixed flags/source names are plain arguments.

Completion formats: Antigravity, Cursor, Copilot, CodeBuddy, Qwen and Devin. Claude/Codex completions remain owned by transcript events. Cursor's `stop` also reports the generation's tokens, which Cursor keeps nowhere else on disk (its own messages store 0/0): the completion event carries them as `data.usage`, the platform appends them to Cursor's turn log in Agent HUD's data folder (`hooks::turn_log`, `clients/cursor/turns-<YYYY-MM>.jsonl`), and the engine reads that log like a client's own. It is the one hook whose facts are kept. Qwen hook timeout values retain the original millisecond convention. Platform callbacks return `{}` for reports, except Antigravity completion callbacks which return `{"decision":"stop"}` as the original entrypoint does; this is not permission approval.

## Event API

`parse_event(source, full_flag, &payload, now_ms)` returns a normalized `HookEvent {source, session_id, at_ms, kind, data}`. It only retains the small facts needed for attention, compaction, origin or successful completion. Origin events do not copy prompts, user-declared host, or navigation targets. The platform may add `data.hostVerified = true`, `data.host`, and `data.navigationTarget` only after verifying process ancestry. Unverified origins imply no running state or destination.

CodeBuddy's original completion adapter optionally reads the latest completed assistant message in at most the transcript's last MiB. `enrich_codebuddy_completion(&mut event, verified_tail)` implements that pure record parsing and stops at a newer user message; the platform alone resolves/validates the transcript and supplies bounded bytes. Without that evidence, the event keeps the original callback-time fallback ID and no fabricated model.

Core's `hook_events::HookEvidence` overlays fresh source-derived session metadata; it does not mutate parser heads or usage. Attention only changes an in-flight turn, compaction is automatic/newer than the source line/at most 300 seconds old, and a completed turn cannot override newer source evidence. Completion-only presentation records explicitly mark usage unavailable; their required numeric `tokens: 0` is a wire sentinel, never a metered contribution. Cursor's tokens reach the ledger through its turn log, not through this overlay.

## Source and validation

The implementation follows the public `agent-hud-native-open` (`Sources/AgentHUDCore`) sources: `PermissionRequest.swift`, `PermissionHooks.swift`, `ClaudeStyleHooks.swift`, `CompletionHooks.swift`, `AttentionHooks.swift`, `CompactionHooks.swift`, `ClaudeSessionOrigins.swift`, `CodexSessionOrigins.swift`, and each public `*HookFormat.swift`.

The golden fixture covers response envelopes, questions and plans, per-client limitations, unknown layouts, owned-command replacement, third-party preservation, install/uninstall idempotence and reporting envelopes. These checks do not write real client settings, install hooks, initiate login, or send native messages. Integration and OS validation remain separate.
