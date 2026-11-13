# Local session data

The broadcast snapshot is the HUD projection, not the history database. Its
`sessionScope: "recent"` includes sessions that are running/waiting or have
presentation activity within 24 hours. `totals`, provider session counts and
interval statistics still cover their complete canonical facts; they are not
recomputed from that smaller array.

The mounted Sessions page queries the same runtime with `sessions`:

```json
{ "scope": "recent", "offset": 0, "limit": 100, "rootsOnly": true }
```

`recent` lists seven days or sessions in flight; `active` lists 24 hours or
sessions in flight; explicit `all` includes retained indexed history. `source`
and `project` are exact filters; an omitted project means all, while `null`
selects unassigned sessions. Search matches the recorded ID, title, source,
model or project path. `parentId` selects direct children and overrides
`rootsOnly`. Sources/projects are facets of the scope and relationship filter,
before the source, project and search filters. Pages are ordered by presentation time
descending, then source, ID and path; limits default to 100 and cap at 200.

`turnBars: true` (the Sessions page) adds to each row what its card draws:
`turnCount`, `turnBars` (the newest 64 turns' new tokens by kind, the
sub-agents' part apart, the compacted turns; 15-minute periods for a log that
marks no prompts), `contextTokens` and `contextFill`. They come from the same
ledger breakdown as `session_detail` and are kept per session until one of its
logs, sub-agents' included, is read further, so a page of unchanged sessions
queries nothing again. Other lists (the HUD, MCP) leave them out.

`Session.updatedAt` is that presentation time: the latest authoritative source
event or active-child event, with the file's recorded update used only when no
authoritative event exists. Scope membership, ordering and non-running day
groups use this time. Raw checkpoint timestamps remain separate for ingestion
and duplicate resolution; this presentation rule does not change token accounting.

A session's `client` is the client that drives it, as its log names it
("Codex Desktop", "Claude Code IDE extension"). Devin CLI and Devin Desktop's
Devin Local write one store, `~/.local/share/devin/cli/sessions.db`, which
names no client: each call there is read once, and a Devin session's `client`
and `origins` come from its latest verified origin hook ("Devin Desktop",
`desktop:devin`; "Devin CLI", `cli:iterm`, or `cli:devin` in Devin Desktop's
terminal). The ACP `usage_update` notifications the desktop app receives carry
running totals of the same calls, not one record per call, and are not read.

The response has `kind: "sessions"`, `revision`, `generatedAt`, `items`, `total`,
`nextOffset`, `sources` and `projects`. A changed revision restarts pagination
at offset zero; clients never combine pages from different revisions.
`sessionsRevision` changes for session facts, canonical selection and rolling
scope boundaries, independently of the snapshot clock. Hidden Sessions windows
stop querying and retain their last display. Partially loaded list summaries
are labelled as covering loaded sessions.

A client whose sessions are known only from its hooks (TRAE, TRAE CN, and any
completion-only client) gets rows from its hook evidence: running from its
prompt and tool-call hooks, idle at its `Stop` or two minutes after the last
renewal, titled by its first prompt, else its folder. A permission
notification newer than the running turn holds it waiting until the next
hook, for up to 30 minutes, since a turn that asked for the user reports
nothing until they answer. They carry `usageStatus: "unavailable"` and no
tokens, and leave the lists like any other session.

Claude Code registers each live process in `sessions/<pid>.json` under its
config home: its pid, when it started (`procStart`), the session it runs, the
version that wrote the file, and what it does now (`status`: `waiting` for the
user, `busy`, `idle`). Every Claude Code process of a registering version
writes one, a headless `-p` run too, and `/clear` and `--resume` rename its
session in place. It removes the file as it exits cleanly; a process killed
outright, as a closed terminal tab kills it, leaves the file behind. Every
collection pass reads the registry (a file again only when it changed) and
asks each pid whether it still runs, the process that started when the file
says (on macOS; signal 0 elsewhere on Unix; on Windows every registered
process still counts as running). The platform layer answers that question
(`Pids`); the registry itself holds no OS code.

What a Claude session's process says comes before the transcript's clock:
- A running process that says the session waits holds it waiting for as long
  as it says so, past 30 minutes; one that says it is busy holds it running
  unless something says it waits.
- A session no running process claims has ended: it is idle, with no requests
  and no attention, when its process left after its last event, or, for a
  root session of the terminal, the desktop app or a headless run written by
  a version the registry shows registering, when its transcript says it runs
  or waits and no running process names it (after a relaunch of Agent HUD,
  say). Sub-agents, other clients and older versions keep the transcript's
  30 minutes.

`session_detail {id}` loads only the requested parser checkpoint and returns
its session, recorded usage entries, parent and children, including relationships
outside the HUD projection. With the usage ledger it also returns `sessionUsage`,
the same breakdown as agent-hud-native-open's `SessionUsage` for a host to publish:
every model and each model's 15-minute periods by agent id, the sub-agents' part
and the kept turns, with times in milliseconds and money as exact decimal
strings. For a requested `turn`, `turnCalls` lists that turn's calls
(each with the `log` it was recorded in) and `agents`: the sub-agents with calls
in it, each with its log, name and type (Claude Code's `agent-*.meta.json`
beside the log, else the log's title), the log that started it, when it began
and was last heard from, and whether it still runs. The session page draws them
as the turn's sub-agent timeline. Existing content stays visible while that session's
facts refresh; unrelated snapshots do not re-read a cold detail. These queries
use committed local facts and do not start collection or provider requests.
Local MCP exposes the same queries as `hud_sessions` and `hud_session`. Synthetic
responses are available only after explicit `--demo`; failures never activate
sample data.

## Where some clients' tokens come from

A session's tokens come from one source for each call; when a client reports
a call in more than one place, the best one counts it and the others give way.

| Client | Sessions and live state | Tokens |
| --- | --- | --- |
| Qoder, Qoder CN, QoderWork | Claude Code's lines in `projects/<folder>/<session>.jsonl` under `~/.qoder` (`QODER_CONFIG_DIR`), `~/.qoder-cn` (`QODERCN_CONFIG_DIR`), `~/.qoderwork` and the older Qoder IDE's `SharedClientCache/cli/projects`; the Qoder desktop app (0.5) runs qodercli's own runtime and writes to `~/.qoder` like the CLI. Sub-agents in `<session>/subagents/`. A session's id is the one its hooks report; it is titled by what the person first typed (`humanInput.text`), and a message another session sent (`origin.kind: "peer"`) is a prompt of its own. Each build is a client of its own. | Each response's usage; Qoder's `input_tokens` holds its cache reads, which count apart. Qoder CLI 1.1.85 writes tokens only for a model of the user's own key or with `QODER_EXPOSE_TOKEN_USAGE` set, and otherwise records `usage.credits`: such a response has no tokens, and its model is the one the reply names (`auto`). Qoder CN's older `QoderCN/SharedClientCache/cache/db/local.db` adds its assistant messages. |
| Copilot CLI | `session-state/<session>/events.jsonl` under `~/.copilot` (`COPILOT_HOME`). | `session-store.db`'s `assistant_usage_events`, one row per call (its input holds the cache's reads and writes, its output the reasoning); else the OTel export's chat spans; else the `session.shutdown` totals, each counted by what it adds. The first store row of a session replaces what the others counted. The Copilot desktop app's `data.db` is not read: its sessions keep `session-state` too. |

## Forks read with their parents' readers

Kilo Code, MiMo Code and TeleAgent keep OpenCode's database, and Oh My Pi
keeps Pi's logs, each under folders of its own. Each is its own source, with
its own session ids (`kilo:`, `mimocode:`, `teleagent:`, `omp:`), read by its
parent's reader:

| Client | Sessions | Left out |
| --- | --- | --- |
| Kilo Code | `$XDG_DATA_HOME/kilo/kilo.db` | Tasks moved in from the VS Code extension (`ses_migrated_*`, no tokens); the JSON files it copied into the database; rows whose data is not JSON |
| MiMo Code | `$XDG_DATA_HOME/mimocode/mimocode.db` | Messages imported from Claude Code, Codex or OpenCode (`external_import`), which are read where they were made; the made-up title "Untitled" |
| TeleAgent | `$XDG_DATA_HOME/TeleAgent/users/<account>/teleagent.db`, one per signed-in account | Nothing of its own; it loads no OpenCode plugin, so its sessions show tokens without live state |
| Oh My Pi | `~/.omp/agent/sessions` | Entries a moved or forked session copied from the one it continues (older than its header); the copies `omp gc --compress` stages in `.omp-gc-compress-*`; calls outside the conversation (`model_usage`) count, without changing the session's model |

Not read: Kilo Code's and MiMo Code's release-channel databases
(`kilo-<channel>.db`, `mimocode-<channel>.db`), `KILO_DB`, `MIMOCODE_DB` and
`MIMOCODE_HOME`; Oh My Pi's profiles, its XDG folders and a
`PI_CODING_AGENT_DIR` it shares with Pi, which Pi reads.

## Sessions kept as a running total

Factory Droid keeps each session in `~/.factory/sessions/<folder>/<id>.jsonl`
beside `<id>.settings.json`. The transcript holds no tokens; the settings file
holds the session's own (`tokenUsage`: input, output, cache reads and writes,
thinking as reasoning; a Task sub-agent's are in its own session, which names
its parent by `callingSessionId`), a running total Droid rewrites in place after
each call. The engine reads the settings file whole each time it changes, as one
call keyed by the session and dated by the file's modification time, so a
reading replaces the last one rather than adding to it: a session's tokens all
fall at its latest change, and none is estimated per reply. The model is the
router's pick (`effectiveFactoryRouterModel`) for `auto`, and a custom model by
its display name without Droid's `[Provider]` label and index. The title,
folder and parent come from the transcript's first line (`session_start`),
which Droid rewrites when the title changes; the transcript is part of the
settings file's stamp, and its last write is the session's activity time.
`archivedAt` archives a session.

## Whole-file and database sources

Most clients append to their logs, which are read from where the last read
stopped. A few write their sessions differently:

- **Cline** keeps each session in `~/.cline/data/sessions/<id>/` since 4.0
  (its VS Code extension and its CLI alike): `<id>.messages.json`, written
  again whole, with each call's `metrics`, and a sub-agent's
  `<agent>.messages.json` beside it, listed under the session. The manifest
  `<id>.json` gives the title, folder and model. Older tasks
  (`tasks/<id>/ui_messages.json` in each VS Code-family editor's
  `globalStorage/saoudrizwan.claude-dev`, and in `~/.cline/data` for JetBrains
  and the first CLI) are read too; a task found under several editors counts
  once, as one session.
- **Roo Code** keeps the same task files in
  `globalStorage/rooveterinaryinc.roo-cline`, with `history_item.json` for the
  title, folder and parent task. Its `tokensIn` counts the cache since v3.29.5,
  Cline's does not (except through its "OpenAI Compatible" provider); both are
  recorded as fresh input apart from cache reads and writes.
- **Goose** keeps one SQLite database, `sessions/sessions.db` in its data folder
  (`~/.local/share/goose`, `%APPDATA%\Block\goose\data` on Windows,
  `GOOSE_PATH_ROOT/data`). Each call is a row of `usage_ledger` (Goose 1.42 and
  later); a pass reads the rows after the last one read, then the sessions
  changed since, for their names, folders, sub-agents' parents and archive
  state. A row carried forward from before the ledger is left out, since its
  time is not its calls'. Older databases give each session's running totals.

None of these layouts has been checked on a machine with the client
installed yet; the samples in `src/crates/agents/tests/fixtures/` are made up
from each client's source.
