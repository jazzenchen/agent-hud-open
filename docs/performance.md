# Collection and rendering ownership

The behavioral reference is the Swift application (`UsageCollector.swift`,
`LogFiles.swift`, `TailLogStore.swift`, `BotActor.swift` and
`PanelSpacingMetrics.swift`).

- A single application-owned Rust service collects data. Settings, Statistics and
  per-screen HUD webviews subscribe to its snapshot stream and bootstrap from its
  cached projection. No native webview owns a polling timer or another runtime.
- Native file callbacks report changed paths and wake that serial collector. File
  events share the original 2-second read spacing; a 3-second stat-only check of up
  to 64 recently active logs covers long-lived descriptors, whose writes macOS reports
  tens of seconds late. A log read before its first line was whole has no event yet and
  counts as active by its last write. Requests and hook reminders do not postpone the
  check. Initial reads, lost events,
  unavailable watches and a 300-second reconciliation list all roots.
- Only changed, outdated or unfinished files last written within the ledger's
  31-day retention enter the incremental read queue; an older file is never
  read again and keeps what it counted. Partial trailing records wait for more
  bytes. File discovery timestamps never
  establish session liveness; source events remain authoritative.
- Numeric hourly contributions update with the affected log. Quiet snapshots
  reuse selected-log and accounting projections rather than reparsing every turn
  timestamp. Changed usage facts, counted-contribution selection, coverage and
  provider changes invalidate those projections. Tool/phase-only checkpoints and
  identical usage observations retain numeric caches; session phase and child
  activity are still resolved against the current clock.
- Success, errors and cached bootstrap use the same serial owner. Native invoke
  return values do not independently write frontend snapshots. Demo startup does
  not create a real runtime or collector.
- SQLite retains complete parser checkpoints without conversation text: turn
  messages and desktop questions' words live only in memory, and a checkpoint
  read back within a run takes them from there. Startup reads persisted lightweight
  heads. Only changed, unfinished or requested detail records decode their full
  parser payload. The LRU admits at most 24 records and 24 MiB of encoded payload;
  this is an admission budget, not a physical-heap bound. Old heads are backfilled
  once without changing parser payloads or their version.
- HUD snapshots contain active/recent sessions, not full history. Complete totals,
  provider counts and Statistics still use every selected contribution. Visible
  Sessions pages requery committed facts when their revision changes; hidden pages
  stop querying. Details decode only their selected checkpoint. See
  [session data](session-data.md) for scopes, paging and live invalidation.
- The application cache moves each projection into an `Arc<Value>`; bootstrap
  shares that projection until serialization. Native mutations return an ack.
  Quiet rounds publish only the collector's `hud:clock`, retaining session-array
  identity in each webview; fact changes or error recovery publish full JSON.
- Frontend views load separately. Statistics does not initialize HUD shaders,
  bot directions, tray listeners or native HUD geometry subscriptions. The hidden
  main window remains the single tray/director/window-plan owner.
- HUD contour observers ignore Bot descendants. Geometry reports merge
  changes, deduplicate identical output and allow one outstanding native request.
- Bots keep the source due times and 24/12/6 fps pacing. Only the selected renderer
  advances; identical pixels and unchanged canvas geometry do not get rewritten.
- `GlowAppearance.resolve` supplies a still neutral rim when there are no quota
  readings. That state does not create a continuous WebGPU renderer. Identical
  palettes also retain the same static-raster input across pointer updates.
- The rim's WebGPU renderer keeps its own display-rate frame loop off. A change
  of size or uniform draws one frame. The soft breathe draws the rim once and
  breathes it as an opacity animation, which WebKit runs in Core Animation, as
  Open's soft glow does; grid styles and the moving effects draw from a timer
  at GlowAnimator's rate (24 fps at the working period, down to 8 at rest).
- A message flight between two marks mounts its elements only while it is under
  way and plays with Web Animations planned up front; idle HUDs draw and run
  nothing for flights.
- Native pointer polling retains its 32/16 ms cadence. It uses
  authoritative canvas readback and a retained window handle, eliminating repeated
  window position/scale/primary-monitor queries and dispatcher clones each tick.
- macOS screen topology invalidation uses the original app-level screen-change
  notification. The native callback signals the existing screen owner without
  measuring or blocking in the notification callback.
- Account usage runs in independent, non-overlapping provider jobs. Client
  protocol and HTTP waits never hold the Runtime/log collector lock. Each result
  enters one account cache and wakes the same snapshot publisher, so an offline
  account cannot stall local sessions or another provider's reading.
- SQLite stores quota observations, complete keyed usage events and numeric
  contribution/model buckets separately from the 128-entry UI session tail.
  Checkpoint and usage updates commit together; stream replacements, replay,
  source rewrites and removals update the same contribution. Reply gaps are
  maintained at write time, not reparsed from JSON on every snapshot.
- The original ledger can seed only exact-path contributions with matching
  codec, file signature and complete retained-history state. Original business
  event keys remain stable for subsequent appends. Unmatched logs retain the
  bounded reader; current-window coverage is distinct from unrelated old pending
  files. Imported facts retain the original history retention boundary.
- Account-exact quota history is imported once into Agent HUD's own SQLite database.
  Source reads use an existing WAL snapshot with read-only shared memory, or a
  signature-checked quiescent main file before opening SQLite. They do not
  initialize missing WAL/SHM files. Failed imports never commit a successful
  marker; repeat imports are idempotent.

## Measured footprint

The README's numbers come from release builds installed and run as a person
would run them, measured from outside the app.

- **What is counted:** the app and the WebKit processes in its resource
  coalition (one web content process per HUD display, the GPU process and the
  networking process), summed. Each process is read from `proc_pid_rusage`
  every 10 seconds for 10 minutes:
  - **CPU:** user plus system time as a percentage of one core, the mean over
    the run.
  - **Memory:** the physical footprint (`ri_phys_footprint`, the figure
    Activity Monitor shows as Memory), in MiB. Where it moved during a run, the
    start and end are given.
  - **Energy:** the energy macOS attributes to the process (`ri_energy_nj`),
    as mean power. The README's working day is that power over 8 hours. The
    display's own power is not part of it.
- **Idle** means no agent wrote to a log during the whole run; **busy** means
  agents wrote in most 10-second intervals. The app is open as usual: Settings
  and Statistics closed, the HUD on screen.

Runs on 2026-10-22, Apple M1 Max, macOS 26, about 8,000 session logs:

| Run | CPU, % of one core | Memory, MiB | Power |
| --- | --- | --- | --- |
| One display, Simple mode, idle | 6.9: app 6.1, HUD 0.5, GPU 0.1, networking 0.1 | 308: app 213, HUD 78, GPU 10, networking 7 | 11 mW |
| One display, Simple mode, busy in 45 of 60 intervals | 7.1: app 6.6, HUD 0.4, GPU 0.1, networking 0.1 | 297: app 207, HUD 72, GPU 10, networking 7 | 12 mW |
| Two displays, busy in 58 of 60 intervals | 27.7: app 11.2, HUDs 4.3 and 7.1, GPU 5.1, networking 0.1 | 589–645: app 231–261, HUDs 158 and 141–169, GPU 51, networking 7 | 282 mW: app 156, HUDs 32 and 52, GPU 40 |

The README's CPU and energy rows give the range between the first and last rows.
Its memory row is Agent HUD's own process, the figure Activity Monitor shows:
about 240 MiB on the afternoon build of 2026-10-22 (238 MiB at 14:05, 241–244
MiB at 14:20 with agents writing), 240–260 MiB while busy; the HUD pages, GPU
and networking processes are listed after it. The one-display
runs predate rendering only on change, which can only lower them;
the two-display run includes it.

