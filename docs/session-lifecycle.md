# Session lifecycle coverage

Usage records and running turns are separate observations. A token counter or a recent file modification does not establish that a whole agent turn is running or complete.

`UsageReport.turns` carries explicit `SessionTurn` observations. Each observation has a stable session and turn identity, a source start time when known, a state (`running`, `completed`, or `ended`), and the timestamp of the latest source event. Reading a cached transcript does not advance that timestamp. Consumers decide how long an observation remains fresh.

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
| Pi | No | No | Assistant stops do not establish that the agent has settled |
| GLM billing services | Not applicable | Not applicable | Execution state belongs to the client using the service |

Older Grok unified logs and older Kimi status logs supply usage information without complete turn lifecycle evidence. Host applications may support additional completion hooks; those are independent of the provider's running-turn records.

DeepSeek packed text, reasoning, and tool-call rows update observation times using their recorded timestamps. Their content is not retained by the transcript index. Inherited history and subagent turns do not become parent running turns. Kimi child-agent events likewise cannot start or finish the main conversation's turn.

Desktop session liveness may additionally use process evidence. It is separate from a turn's last recorded observation: a quiet process does not manufacture a fresh transcript event, and a disappeared process does not prove successful completion.
