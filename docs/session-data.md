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

The response has `kind: "sessions"`, `revision`, `generatedAt`, `items`, `total`,
`nextOffset`, `sources` and `projects`. A changed revision restarts pagination
at offset zero; clients never combine pages from different revisions.
`sessionsRevision` changes for session facts, canonical selection and rolling
scope boundaries, independently of the snapshot clock. Hidden Sessions windows
stop querying and retain their last display. Partially loaded list summaries
are labelled as covering loaded sessions.

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
