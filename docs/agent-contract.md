# The agent contract

Every client Agent HUD follows is described once, in its folder under
`src/crates/agents/src/`, and every client follows the same contract: what it
reads, how its sessions and accounts are found, which hooks it takes, how its
requests and replies work, and how it reports what changed. This document is
that contract.

## One shape for every agent

Each client is an `Agent`, and every behaviour is an optional section. A
missing section means the client lacks that behaviour; the engine and the UI
read that from the descriptor rather than from special cases.

```rust
pub struct Agent {
    pub id: &'static str,
    pub name: &'static str,
    pub sources: PerOs<&'static [Location]>, // see "Behaviour per operating system"
    pub transcript: Option<&'static dyn Transcript>, // usage and live facts
    pub account: Option<&'static dyn Account>,       // quota, balance, plan
    pub hooks: Option<&'static dyn Hooks>,           // what its hooks report
    pub requests: Option<&'static dyn Requests>,     // permissions, questions, plans
    pub replies: Option<&'static dyn Replies>,       // words into a session
}
```

An agent describes. The engine (core, platform, runtime) does.

| Behaviour | The agent says | The engine does |
| --- | --- | --- |
| Transcripts and usage | Which files are its logs, how one is read (JSON lines, SQLite, protobuf, batches), and the facts a read yields: usage events (key, model, time, tokens), turns, title evidence, live facts (turn started or ended, tool call, question asked) | Offsets and checkpoints, deduplication, the ledger |
| Session state | Facts only. Its exceptions are data, for example "no lifecycle in the logs: idle" | One state machine for every agent: running, waiting, idle, unverified (quiet after 2 min, abandoned after 30 min) |
| Quota and account | `read(host) -> Reading`: quotas, balances, accounts, each with the time it was observed. Its cadence: interval, whether local work moves it, which sessions spend it | Scheduling, the serial collection gate, retries, quota samples in the ledger |
| Hooks | The events it can send (permission, completion, attention, session origin, compaction), how its settings file is patched, and how a payload becomes a hook fact | Installation, the OS channel, verifying the caller's process |
| Requests | How a payload becomes a request (kind permission, question or plan; its questions, plan, suggestions); which decisions it accepts (allow, deny, always, answer, approve plan, keep planning, leave); how a decision becomes its reply, where no opinion means no bytes | One owner for every waiting request: tickets, the shared hold time, expiry, deliveries, the reviewer gate |
| Cards | Which cards it can answer, and through which channel: the hook callback, a native responder, or a reply into the session | Every card is a request view of one shape; an answer goes back through the channel the request came from |

## Behaviour per operating system

The same agent behaves differently on macOS, Windows and Linux. These things differ:
- where its files are;
- what its executable is (`claude` or `claude.cmd` or `claude.exe`);
- how its hooks are launched;
- where its credentials are kept;
- which native channels it has.

Today all three hosts share one list of home-relative paths. Windows and Linux
report "no secret" for every credential, so a reader quietly falls back to a
file. The contract makes each difference explicit and prevents the failures
that are known in advance.

**Every OS-dependent fact is stated for every OS.** It is a struct with three
required fields rather than a map with a default, so the compiler refuses a
descriptor that forgets one:

```rust
pub struct PerOs<T> { pub macos: Support<T>, pub windows: Support<T>, pub linux: Support<T> }
pub enum Support<T> { Verified(T), Unverified(T), Unsupported(&'static str) }

// cursor/mod.rs: the same file under each OS's application data folder.
sources: PerOs {
    macos: Verified(&[Location::app_data("Cursor/User/globalStorage/state.vscdb")]),
    windows: Unverified(&[Location::app_data("Cursor/User/globalStorage/state.vscdb")]),
    linux: Unverified(&[Location::app_data("Cursor/User/globalStorage/state.vscdb")]),
},
```

`PerOs` covers these facts:
- source locations;
- executables and the process names that identify the client;
- the hook launch contract (an argv form on Windows, never a POSIX command string);
- credential places, in the order they are tried;
- native channels;
- opening a session.

**Support decides what runs.**
- *Unsupported:* the behaviour is never attempted on that OS, and the UI says "not supported on Windows yet" from the descriptor.
- *Unverified:* reading runs. Anything that writes outside Agent HUD, such as installing hooks or patching a client's settings, waits for *Verified* or an explicit opt-in.
- Raising an entry to *Verified* needs fixtures from a real machine. That entry is how a Windows contributor finds open work and closes it.

**The host returns why, not just nothing.** `Host` results are `Found`,
`Missing`, `Unsupported`, `Denied` or `Failed`. A credential store this OS
lacks is *Unsupported*, not *Missing*, so a reading reports it instead of
reading a fallback silently. Fallbacks are declared in the descriptor, for
example `[Keychain("gh:github.com"), File(".config/gh/hosts.yml")]`, and the
reading records which one answered.

**The engine handles the known hazards once, in its per-OS files, so no agent can get them wrong:**

| Hazard | Where | Guard |
| --- | --- | --- |
| A log the client holds open | Windows | Open it with full sharing; never lock or rename a client's file |
| CRLF lines, BOMs | Windows | The line reader accepts both |
| `.cmd` and `.ps1` shims instead of the real program | Windows | Resolve the real executable before checking a hook caller's process or starting an engine |
| Shell quoting in hook commands | Windows | Argv forms only; an unknown launch contract is refused, as today |
| Case-insensitive and long paths | Windows, macOS | Compare paths the OS's way; use long path forms on Windows |
| Privacy prompts from broad scans (App Data, Keychain) | macOS | Read only declared locations and declared Keychain items, with a time limit; never scan the home folder |
| No file events on network drives, inotify limits | All, Linux | The stat-only check stands in |
| WSL beside Windows | Windows | A separate source environment with its own identity, never merged with the Windows paths, so usage is not counted twice |
| Wayland | Linux | HUD capabilities report it; agents are unaffected |

**Tests cover every OS on every machine.**
- `agents` is pure, so its fixture tests run each OS's table on every runner against a fake `Host` per OS. For example, they cover Windows paths with drive letters, CRLF logs, `.cmd` shims, and a missing credential store.
- OS-specific samples live in `tests/fixtures/<id>/<os>/`.
- The native CI matrix still runs platform's real adapters on each OS.

**Checks.** `check:clients` also refuses `cfg`, `target_os` and
`std::env::consts::OS` inside `agents`. It generates the agent × OS × behaviour
support table from the descriptors. Docs, the capability report and the UI
read that one table.

## Changes are events

Agents and engine parts report what changed, never "read everything again":

```rust
pub enum Change {
    LogGrew { path: PathBuf },
    LogGone { path: PathBuf },
    AccountRead { agent: &'static str, observed_at: i64 },
    HookFact { agent: &'static str, session: String, fact: HookFact },
    RequestOpened { id: String },
    RequestClosed { id: String },
    Setting { key: String },
    Due { at: i64, why: Due },
}
```

- **Producers:**
  - the OS file watcher;
  - a stat-only check (size and modification time, no read) for logs held open, whose events can come late;
  - the hook channel;
  - the account scheduler;
  - the request owner;
  - settings.
- **A pass does only what its changes call for.** It reads the logs named and applies the reading. Session rows are reused while no session input changed, and pages are told only when the snapshot changed. Between passes, a stat-only look every 3 s checks the logs held open (a session read before its first line was whole counts, by its last write); with no change there is no pass and no render.
- **Time-based changes are deadlines.** Each part says when its next change falls due:
  - a session going quiet or abandoned;
  - a request expiring;
  - a heartbeat lapsing;
  - a session leaving the 1-day or 7-day list;
  - a quota resetting;
  - a forecast instant, such as when a quota runs out (`Due { at: exhausts_at, why: Forecast }`);
  - the next account reading.

  One timer waits for the earliest. Until every deadline is known, a 30 s safety refresh stands in.
- **Values that move with the clock become instants (planned).** The snapshot will carry "runs out at 14:32" and "resets at 18:00", and pages will only format the distance from now. Core keeps computing every number; until then, a once-a-minute metrics clock holds them steady between changes.
- **A forecast is recomputed on time.** Core recomputes it when its instant is reached, or when a reading or usage changes, whichever comes first. Otherwise an instant fixed at the last data change goes stale: a quota read at 12:00 with 30% left, used at 20% an hour, says "runs out 13:30", and with no use since it must not still say so at 14:00.

The host follows the same rule outside agents: the pointer and the screens are meant to be system events rather than polls.

## Hooks, requests and replies

**Data, not `dyn` traits.** The sections are structs of facts, with function
pointers where a client's behaviour is code (`Completion::read`, `enrich`,
an account `read`). That is how steps 0 to 2 built them. The support table,
`check:clients` and the UI can read facts, and a trait object would hide them
behind code.

The request owner (`core/src/requests.rs`) already names no client. It holds
`agents::Request` values, and each client's questions, plans and replies
live in `agents/src/hooks/request.rs` and its descriptor. What remains is
engine code that decides by client name. It goes in five slices:

1. **Hook facts**:
   - which hooks a client installs (`Hooks::flags`);
   - which clients report through an observer;
   - which completion is enriched from the transcript;
   - what a completion callback is answered (`Completion::reply`, Antigravity's `{"decision":"stop"}`);
   - whose transcript settles a waiting request (`Permission::settles_in_transcript`);
   - the reviewer lookup by the request's own source;
   - which hook events core accepts from whom.

   This also fixes Qoder Work: platform spelled it `qoderwork`, so its permission requests never got the transcript watch.
2. **Where the hooks live**. `Hooks::settings`
   is a `Location`: the folder variable (`CLAUDE_CONFIG_DIR`, `CODEX_HOME`,
   `CODEBUDDY_CONFIG_DIR`, `QWEN_HOME`, `GEMINI_CLI_HOME`, `COPILOT_HOME`) and
   each client's file, which `platform/src/hooks/install.rs` resolves.
3. **Native channels**. `Agent::channel` names each:
   - `codex::DESKTOP`, the desktop app's IPC;
   - `antigravity::LOCAL_SERVICE`, its language server;
   - `deepseek::PLUGIN`, Agent HUD's Harness plugin and the folder it shares
     with the app (`platform/src/deepseek_plugin/`); Harness 0.1's web
     question stream, `deepseek::QUESTIONS`, still runs beside it.

   Each says what it carries (requests, quota) and on which OS it runs.
   Platform keeps them in `platform/src/clients/<id>/`, starts each only where
   it runs, and takes the client's id and name from the descriptor.
4. **Telling a client's process**.
   - `Agent::program` names the command-line program.
   - `agents::process` holds the caller types: `Ancestor` and `Caller` (the kernel-reported parents and the callback's own environment) and `Origin`.
   - Each client's rules live in its folder. `claude/process.rs` holds Claude Code's argument grammar, the apps that host it, its entrypoints and headless runs. `codex/process.rs` holds Codex's invocation grammar and its desktop app.
   - `Hooks::origin` and `Permission::unattended` name the rules.
   - Platform's `hooks/origins.rs` gathers the caller and applies the result, including the iTerm target. Codex's daemon terminal lookup moved to `clients/codex/`.

5. **Replies**.
   - `Agent::replies` says which client takes words into a session, what carries them, and which setting turns answering on: Claude Code, Agent HUD's plugin, `claudeAnswers`.
   - Core's `SessionReplies` stays the host's carrier.
   - The Claude desktop app's questions stay in `core/src/clients/claude_desktop.rs`, since they hold core's cards and tickets. They take Claude's id and the setting from the descriptor, as platform's plugin carrier does.

## Locations as data

`Location` says where a client keeps a file or folder:
- `base`: home, application data, or the XDG data or config folder;
- `env`: folder variables, the first set one winning;
- `folders`: alternative folder names, the first existing one winning;
- `profiles`: profile folders matched by a name prefix;
- `path`: the file or folder inside;
- `not_if`: a folder a variable may name for another client.

Locations resolve through `Places` (exists, same, names): platform answers from
the disk, and account readers from their `Host`.

1. **Sources, hooks and observers**.
   - `Agent::sources` lists where each client's sessions are, per OS. Platform's
     source roots come from those, in the agents' order, each path once.
   - `Hooks::settings` places each hook settings file.
   - `Observer` holds the observer plugins' turn folders, script paths, markers
     and scripts (moved to `agents/src/<id>/observer.*`).
   - A test pins every default root; before the switch, today's roots and the
     descriptors' matched in every environment tried.
2. **Discovery**. `Agent::discovery` says what tells a
   client is here:
   - storage, settings, session folders and usage files, as locations;
   - command names, app names and bundle identifiers;
   - credential variables;
   - its own rule where data is not enough: GLM's plan routed through Claude
     Code, OpenCode or Pi (`agents/src/glm/discovery.rs`).

   `platform/src/discovery.rs` only looks. A side-by-side run against the old
   probes matched for every client, with one exception: with several OpenClaw
   folders, the evidence now lists storage before settings. One shared
   `Location` settles two old disagreements with the source roots:
   - with `OPENCLAW_STATE_DIR` set, profile folders are not probed;
   - a `CODEBUDDY_CONFIG_DIR` that links to WorkBuddy's folder is WorkBuddy's.
