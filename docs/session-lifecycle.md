# Session lifecycle coverage

Usage records and running turns are separate observations. A token counter or a recent file modification does not establish that a whole agent turn is running or complete.

`UsageReport.turns` carries explicit `SessionTurn` observations. Each observation has a stable session and turn identity, a source start time when known, a state (`running`, `completed`, or `ended`), and the timestamp of the latest source event. Reading a cached transcript does not advance that timestamp. Consumers decide how long an observation remains fresh.

Every execution client uses **Settings → Agents → [Agent] → Live status**. The shared `Settings.liveStatusEnabled(for:)` policy controls desktop running indicators. Turning it off takes effect without changing the collector, session history, token statistics or quota windows. Billing-only services do not have a live-status switch. Hosts that add completion reminders, live-status relay, or device-settings synchronization apply this same preference in their own services; those services are not part of the standalone open application.

Collectors normalize their own formats into the same `LiveSession`, `SessionTurn`, `SessionCompletion` and usage-event models. The desktop and optional host services consume those models. The switch permits available status observations; it does not manufacture lifecycle support for sources whose logs only provide usage.

| Client / source | Running turns | Terminal turns | Evidence |
| --- | --- | --- | --- |
| Claude Code | Yes | Yes | User prompt, assistant/tool observations, final response or interruption |
| Codex Desktop / CLI | Yes | Yes | Identified task start, subsequent events, task completion or interruption |
| DeepSeek Harness v0 | Yes | Yes | `turn/start`, streaming/tool events, `turn/end` |
| Grok CLI session updates | Yes | Yes | User/agent/tool updates and `turn_completed` |
| Kimi Code `agents/main/wire.jsonl` | Yes | Yes | First `step.begin`, loop events and `turn.ended` |
| Cursor | No | No | Current provider supplies usage observations |
| Antigravity | No | No | Current provider supplies usage observations |
| OpenCode | No | No | Current provider supplies message usage observations |
| Pi with Agent HUD observer | Yes | Yes | Native `agent_start`, `agent_settled` and `session_shutdown`; assistant stops alone do not finish a run |
| GLM billing services | Not applicable | Not applicable | Execution state belongs to the client using the service |

Older Grok unified logs and older Kimi status logs supply usage information without complete turn lifecycle evidence. Host applications may support additional completion hooks; those are independent of the provider's running-turn records.

DeepSeek packed text, reasoning, and tool-call rows update observation times using their recorded timestamps. Their content is not retained by the transcript index. Inherited history and subagent turns do not become parent running turns. Kimi child-agent events likewise cannot start or finish the main conversation's turn.

Desktop session liveness may additionally use process evidence. It is separate from a turn's last recorded observation: a quiet process does not manufacture a fresh transcript event, and a disappeared process does not prove successful completion.

The standalone host prepares required observers when monitoring starts. For Pi, it installs or updates its own extension when the Pi directory exists; existing Pi processes need a one-time `/reload` after installation. New Pi processes load the extension automatically. Changing **Live status** does not install or remove extensions and needs no reload. Manual setup remains available as `AgentHUDOpen --install-pi-observer`. Both the installer and reader honor `PI_CODING_AGENT_DIR` (default `~/.pi/agent`).

`LiveSession.observedAt` records when the collector last checked desktop activity, including any process evidence. Retaining or decoding the session does not advance it. A running observation older than 120 seconds remains in the session history with a status awaiting refresh; it no longer drives the running indicator. A failed read does not create a successful completion or an artificial session end.

The observer writes metadata-only snapshots to `agent-hud/turns/` inside that directory, including turns whose first response has not yet been persisted. It keeps retries, compaction and queued continuations running until `agent_settled`. Only a successful final response emits a completion reminder; errors, cancellation and shutdown end activity without claiming success. While a run remains active, the observer reports its state every 15 seconds. If Pi exits without a shutdown event, the last observation stops being considered live after 120 seconds. Re-reading a file does not refresh that timestamp. Observer files are retained for seven days.

Token totals continue to come exclusively from Pi's message transcripts, with the existing fork/request deduplication. Installing the observer does not replay old completion reminders or create extra usage events. Without the observer, Pi still supplies historical sessions and token usage.
