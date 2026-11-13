# Account usage ownership

`platform::accounts` reads accounts independently of transcript ingestion. Each
provider has one non-overlapping background job. Network and child-process waits
do not hold the Runtime mutex. A completed reading updates the shared cache and
wakes the application collector; every native window receives that projection.

The implementation follows the original Swift account readers:

| Provider     | Existing source                                     | Read operation                                                       |
| ------------ | --------------------------------------------------- | -------------------------------------------------------------------- |
| Claude       | Installed Claude Code, its current folder and selected additional folders; Claude Code's own usage record in each folder's `.claude.json`; for the current folder, the Claude desktop app's claude.ai sign-in | The usage record while it is under a minute old; else SDK `get_usage`, hooks disabled, `skip_behaviors`, no prompt; signed out or not installed: claude.ai's usage page with the desktop app's sign-in; when every way fails, the record's windows as old as they are (see [Claude Code's usage record](#claude-codes-usage-record)) |
| Codex        | Installed Codex, its current home and selected additional homes | `initialize`, `account/rateLimits/read`, `account/read`, no turn |
| Pi / ChatGPT | Existing, unexpired Pi OAuth home                   | Official ChatGPT usage endpoint; no OAuth flow or credential rewrite |
| Cursor       | Read-only Cursor account SQLite database; a sealed sign-in (`cursor-safe-storage:v1:`) opens with Cursor's "Cursor Safe Storage" keychain item, which macOS asks about once (Electron files it under "Cursor Key", older builds under "Cursor") | Current usage-summary endpoint and billing-cycle reset; for an account merged into SuperGrok (plan `super`), or when the summary is refused, the period Cursor itself shows (`DashboardService/GetCurrentPeriodUsage`): its percentages where it reports them (a merged plan), else the cents spent of each limit (a plan metered in dollars), as Cursor's usage bar works them out; a meter with neither is not shown |
| Grok Bot     | The Bot's cache (`sand-client-persistence`), and Cursor's sign-in when it is the Bot's account | The Bot's weekly meter (`GetSandUsageStatus`) with Cursor's sign-in to the same account, which also names the account; else the Bot's cache. A server that misses an answer is a passing failure, so the last reading stays rather than an older cache; any other server failure falls back to the cache. Each bot's daily tokens from `GetDailySpendByCategory`, none for a merged account |
| DeepSeek     | Harness settings and versioned credential reference; the key Claude Code (`ANTHROPIC_BASE_URL`) or Codex (`[model_providers.*]` `base_url`) sends to `api.deepseek.com`, and the API key of Pi's `deepseek` provider in `auth.json`, or of a Pi provider whose `models.json` `baseUrl` is on that host (Open #64) | Official API balance endpoint and response currency; each client's own key is its own balance, labelled with the clients that send it, and a key Harness shares with them is read once |
| Kimi, GLM, OpenCode Go | Keys other clients hold: the environment, Claude Code's settings files (as for relays below), OpenCode, Pi, Kimi Code, WorkBuddy | Each plan's own usage endpoint |
| MiniMax      | Plan keys other clients send to MiniMax's own API: `MINIMAX_CODING_API_KEY`, `MINIMAX_API_KEY`, Pi's `MINIMAX_CN_API_KEY`, Claude Code's `ANTHROPIC_BASE_URL`, Codex's `[model_providers.*]`, OpenCode's and Pi's sign-ins; pay-as-you-go keys (`sk-api-`) are left out | `GET /v1/token_plan/remains` on the key's platform (`www.minimax.io`, or `www.minimax.cn` for China), else the older Coding Plan `/v1/api/openplatform/coding_plan/remains`; a key no client places on a platform is tried on the other when the first refuses it. Each model family's 5-hour and weekly windows; prompts used of the total where the plan counts them; a key with no active plan (status 2062) shows nothing, with a notice, rather than a failed reading; read every five minutes whatever this computer does, since clients not read here and other devices spend the plan too |
| Volcengine Ark | An installed `arkcli`, signed in through its own login; Agent HUD reads none of its credentials | `arkcli usage plan --format json`, with implicit updates off (`ARKCLI_NO_UPDATE_NOTIFIER=1`): Coding Plan and Agent Plan, personal and team, 5-hour, weekly and monthly windows; a plan the CLI could not read is named in a notice; no model call; read every five minutes whatever this computer does, since clients not read here and other devices spend the plan too |
| Grok CLI     | The CLI's own xAI sign-in (`auth.json` in `GROK_HOME`, else `~/.grok`), never refreshed or rewritten; an expired one keeps the last credits until the CLI runs again (it renews its token on every run), and only a missing or refused sign-in counts as signed out | The CLI's billing endpoint (`cli-chat-proxy.grok.com/v1/billing?format=credits`): the share of the period's credits used, on-demand credits of their cap when reported, the plan from `/v1/settings` |
| Copilot      | Once Copilot quota reading is on: the GitHub CLI's sign-in (environment, credential store, `hosts.yml`), then the one GitHub's Copilot clients keep (`github-copilot/apps.json`, older `hosts.json`); a token GitHub refuses gives way to the next | `copilot_internal/user`; premium requests beyond the plan as an estimated overage in US dollars (US$0.04 each, GitHub's published price) |
| Hermes / Nous Portal | Hermes's own Nous Portal sign-in (`auth.json` in `HERMES_HOME`, else `~/.hermes`: `providers.nous`, else the credential pool; else `shared/nous_auth.json`), never refreshed or rewritten, since Nous rotates its refresh tokens and a reused one signs Hermes out; an expired access token is never sent: the reading is transient, so the last credits stay as they were for 10 minutes and then show stale with the reason that Hermes renews its Nous Portal sign-in the next time it runs and the credits update then; no Nous sign-in shows nothing | `GET /api/oauth/account` on the stored portal only when it is `https` on `nousresearch.com` or a subdomain, else `portal.nousresearch.com`: the month's subscription credits used of the grant (resetting at the period end), the top-up credits in US dollars, the plan; a refused token is a sign-out |
| Antigravity  | Connected CLI status-line reports and all running authenticated language servers | Identified quota callbacks, serial loopback `GetUserStatus`, anonymous `/usage` fallback |
| Factory Droid | A Factory API key only: `FACTORY_API_KEY` in the environment, else in `~/.factory/.env` (CodexBar's place for it); Droid's own sign-in and the web app's cookies are never read. No key leaves a notice, not a sign-out; a key Factory refuses (401/403) is signed out | `GET api.factory.ai/api/billing/limits`, as the CLI's `/usage` does: the share used of the five-hour, weekly and monthly windows of the Standard pool and, where reported, the Droid Core pool (a positive `secondsRemaining` sets the reset, else a future `windowEnd`; a window whose end passed reads as reset), and the extra usage balance in US dollars. A plan with no windows there shows a notice. Shapes follow CodexBar's Factory probe (MIT) |
| Cline (ClinePass) | Cline's own sign-in in `providers.json` (`providers.cline.settings`, under `CLINE_DATA_DIR`, else `CLINE_DIR`/`~/.cline` `data/settings`): its browser sign-in's access token, sent as `workos:<token>`, else a saved API key, else `CLINE_API_KEY`; never refreshed or rewritten. An expired token keeps the last reading until Cline runs again; a 401 is a sign-out, a 403 or 404 an account with no plan to read | `api.cline.bot/api/v1/users/me/plan/usage-limits`: the 5-hour, weekly and monthly share used and their resets |
| Qoder        | A sign-in the user makes on qoder.com or qoder.com.cn in Agent HUD's own window (see below); Qoder, Qoder CN and QoderWork spend it. The CLI's and the app's own stored sign-in (`.auth/user`, encrypted) is never read | `GET /api/v2/me/usages/big_model_credits`: the plan's model credits, shared credits added, and their reset |
| WorkBuddy    | A sign-in the user makes on www.workbuddy.cn in Agent HUD's own window; WorkBuddy's own stored sign-in is encrypted and never read | `POST /billing/meter/get-user-resource-summary`: credits and reward credits left of their cycle, the plan; the reset from the paid, then free, package lists |
| TRAE         | A sign-in the user makes on www.trae.ai (TRAE) or www.trae.cn (TRAE CN) in Agent HUD's own window; TRAE's own stored sign-in is encrypted and never read | Each reading trades the session for a short-lived token (`POST /cloudide/api/v3/common/GetUserToken`, never kept), then `POST /trae/api/v{1,2}/pay/user_current_entitlement_list` with `Authorization: Cloud-IDE-JWT`, whose answer is counted as TRAE's own client counts it (`ICubeUsageService`): credits as what is left of every pack's `credits_limit` (the remainder TRAE CN's title bar shows; the site's `usage_summary` when no pack has one), else dollar usage as a share of the plan's basic plus bonus allowance, reset a second after the plan's period ends (no reset for credits). The plan is the highest tier among the packs (Ultra, Pro+, Pro, Lite, Free); packages, pay-as-you-go and promo codes are no part of it. A limit of -1 is no limit: the plan shows with no share. International accounts are asked at `ug-normal.trae.ai`, then, if it refuses the session, `ug-normal.us.trae.ai`; TRAE CN at `api.trae.cn` |

Each provider states its own facts in its folder under `src/crates/agents/src/`
(`agents::account::Provider`, listed in `agents::ACCOUNTS`): how it is read,
whether this computer's work moves its windows, which session sources spend
them, whether a window a reading leaves out keeps its last value, which rows a
client reports by itself, and the vendor's status page. A reading that fails
says why (`Unread`): the agent signed out, the reading only missed an answer
(offline, a timeout, the service busy or asking to wait), or anything else.
What a 401 means is the provider's to say.

Everything else is the same for every provider:

- A reading that only missed an answer, while the last good one is under 10
  minutes old, changes nothing on screen. Any other failure shows at once with
  its reason, and the last reading stays, marked stale.
- A failed reading may bring what the agent itself last recorded
  (`Reading::failure`). It is still that failure in every way above; its
  windows and accounts take the place of the ones shown only where they are
  newer, or not shown yet, and each keeps the time it was read.
- After a failed reading the next try waits 60 s, then twice as long each time,
  up to 15 minutes. A refresh the user asks for, or this computer's work
  finishing, tries at once. A service's `Retry-After` is followed (at most an
  hour), even for a refresh.
- An agent that signed out is told on the island once, at most once a day,
  again after it was signed in in between. The first look after launch only
  notes how things stand. The Mac posts no system notification.
- A vendor status page (Statuspage `summary.json`) is read every five minutes
  in the same one-at-a-time queue, after any account reading that is due, with
  no credential. The agent's quota block shows the page's word only while one
  of the components the provider names is not operational.

The client retains only projected account identity/plan, percentages, reset
times, balances and fixed diagnostics. Authentication tokens, headers, raw responses and
credential documents are never sent to React or included in error messages.
HTTP redirects are disabled. Missing or expired authorization remains explicit;
installation detection does not imply an available quota reader.

The Antigravity loopback client bypasses system proxies and accepts the local
service's self-signed certificate only for a fixed `127.0.0.1` endpoint discovered
from that service's process. External account requests retain normal TLS checks.

The default native launch reads real accounts. Explicit `--demo` skips the real
runtime and all account jobs. Closing Settings or opening a second HUD does not
start another account reader.

## Web sign-in

Some quotas are shown only on the vendor's own site: Qoder's model credits,
WorkBuddy's credits and TRAE's usage. A provider states where the user may sign in for one
(`agents::account::WebSignIn`: each site's host, name and page). Nothing about
it starts by itself:

- The block of a detected agent says "Sign in to show the quota" with a button
  per site, and the agent's settings page lists the sites with **Sign in** and
  **Sign out**. Only those buttons open the window.
- The window is Agent HUD's own, on the vendor's page, private (nothing the
  site stores outlives it), limited to `https`, and none of Agent HUD's
  commands answer it (`host/src/web_sign_in.rs`, `foreign`).
- While it is open, each new set of the site's cookies is tried once with the
  provider's own reader, in the collection queue, as the only session that
  reading sees. Once the reader reads the quota with them, the cookies and the
  window's browser identity are kept (`account-credentials/web/<provider>@<host>.json`,
  owner-only), the window closes and the provider is read again. Closing the
  window first keeps nothing.
- The interface is told only where a session is kept (`webSignIn`), never the
  session. Agent HUD never reads a browser's or another app's session for these.
- A site that refuses the kept session makes the reading "signed out": the
  sign-in reminder appears once a day and the block offers the sign-in again.
  **Sign out** forgets the session.

## Quota display

**Settings → General → Usage display** selects **Used** (the default) or
**Remaining**. Percentages, quota bars and rings use that choice across the HUD,
expanded panel, reminders and menu bar. For example, 80% consumed becomes 20%
remaining. The preference is saved locally as `settings.quotaDisplay`.

Risk colors, low-quota alerts, burn rates and stored readings continue to use
the actual consumption. Unknown quota remains unknown, and currency balances
and token counts are unchanged.

## Multiple Codex accounts

In **Settings → Agents → Codex → More Codex accounts**, add the
`CODEX_HOME` directories for accounts already signed in through Codex, one per
line. Absolute paths and `~/` paths are accepted. The current `CODEX_HOME`
(or `~/.codex` when unset) remains included. Clear the list to return to that
default account only. The list is saved locally as `settings.codexHomes`.

The account worker reads the homes sequentially, using the selected
`CODEX_HOME` only for that child process. It resolves duplicate directories and
merges repeated subscription identities, including Pi's ChatGPT sign-in. The
same email in different workspaces remains separate. A failed home reports a
notice while successful homes still appear; if every reading fails, the last
reading remains marked stale. Saving a changed list discards readings from the
previous list, including results still in flight.

These additional homes provide subscription quotas. They do not change the
client's active account, install hooks, or add transcript directories to token
statistics.

## Multiple Claude Code accounts

In **Settings → Agents → Claude Code → More Claude Code accounts**, add the
`CLAUDE_CONFIG_DIR` folders for accounts already signed in through Claude Code,
one per line. Absolute paths and `~/` paths are accepted. The current
`CLAUDE_CONFIG_DIR` (or `~/.claude` when unset) remains included. Clear the
list to return to that default account only. The list is saved locally as
`settings.claudeHomes`.

The account worker reads the folders sequentially, running Claude Code's engine
with the selected `CLAUDE_CONFIG_DIR` only for that child process. Claude Code
uses its own sign-in for that folder; Agent HUD reads no credentials. Each
folder's account is the one its `.claude.json` names, and a login that names
none is keyed by its folder. Duplicate folders, including the default one, are
read once, and an account signed in from two folders appears once. A failed
folder reports a notice while the other folders still appear, with the windows
Claude Code last recorded in that folder, marked stale; if every reading fails,
the last reading remains marked stale. The Claude desktop app's sign-in
stands in only for the default folder, never for an added one. Saving a changed
list discards readings from the previous list, including results still in
flight.

As with Codex, these folders provide subscription quotas only. They do not
change Claude Code's active account, install hooks, or add transcript folders to
token statistics.

## Claude Code's usage record

Claude Code keeps its last usage answer in its global configuration,
`.claude.json` (in `CLAUDE_CONFIG_DIR` when it is set, else in the home
folder), as `cachedUsageUtilization`: when it was fetched (`fetchedAtMs`), for
which account (`accountUuid`), and the answer itself, the same object
`get_usage` returns as `rate_limits`. Claude Code writes it after each usage
read made with its own sign-in (`/usage`, or the `get_usage` Agent HUD asks
for), at most once a minute, and clears it when it signs out. The Claude
desktop app's engine leaves it alone. Agent HUD reads the record only for the
account the same file names (`oauthAccount.accountUuid`), never one that names
another account or none, and reads no identity field or credential to do so.

- **Under a minute old**, the record is the reading, and Claude Code's engine is
  not started. Claude Code's engine answers `get_usage` from the record itself
  while it is that young, so this is what the engine would answer. A longer
  bound would mostly read back Agent HUD's own previous answer, since Agent
  HUD's readings are what write the record most of the time, and would halve
  how often the quota is really read.
- **When every way fails** (Claude Code signed out, claude.ai's browser check,
  an engine that does not answer), the record's windows stand in, up to 30
  days old, as a failed reading's last windows do: marked stale and read when
  Claude Code read them, with the failure's reason followed by how old they
  are ("Showing Claude Code's own reading from 2h ago"). The failure stays
  what it was, so a signed-out Claude Code is still told on the island to sign
  in again. When the reading was the desktop app's, the record stands in only
  if the desktop app last signed in to the same account (its
  `lastKnownAccountUuid`); another account's record is never shown in its
  place.

Each window gets the id the engine's reading gives it: the session, the week
across models, and a model's week (`seven_day_<family>`, or a `limits[]` entry
of kind `weekly_scoped` naming a model). Windows that are null or unknown,
extra usage, and weeks scoped to a surface rather than a model are left out,
never shown as 0. The plan comes from `oauthAccount` (`organizationType`, then
the rate-limit tier). Reading the record this way follows QuotaView
([third-party notes](third-party.md)).

## Antigravity accounts

In **Settings → Agents → Antigravity → Antigravity CLI accounts**, add the
CLI's `settings.json` files, one per line, then select **Connect accounts**.
**Add default configuration** supplies
`~/.gemini/antigravity-cli/settings.json`. Restart each connected CLI to begin
receiving its account reports. The paths are saved as
`settings.antigravityCliSettings`; they identify CLI settings, not credential
stores or invented profile directories.

Agent HUD connects the CLI's official status-line callback, preserving an
existing status-line command and its visible output. Clearing the list and
connecting again disconnects the callbacks and restores the original settings.
The installer currently supports macOS and Linux. On Windows, the connection
controls report that callbacks are unavailable and leave CLI settings untouched.
The callback retains only email, plan, quota percentages, reset times and the
observation time. It does not retain the callback's workspace, conversation or
transcript fields. No login, credential switching, or model request is needed.

Each reported email appears as a separate account. Switching accounts in a
connected CLI retains the earlier account's latest observation; multiple
connected CLI configurations work the same way. The same email is deduplicated.
Observations older than five minutes, or past their reset, remain visibly stale
until the CLI supplies a new report. Cached reports expire after 30 days;
disconnecting a configuration removes its readings from the selected sources.
Unknown percentages remain unavailable.

Every readable running Antigravity language server also contributes its
email/team account. Identified CLI weekly and five-hour windows take precedence
over that same account's native model-family totals, whose window lengths are
unknown. The original CLI observation time remains visible even when a native
service is running. Different native team identities remain separate because
the status-line payload provides no team identity.

When no identified account is available, a running `agy` can supply anonymous
quota through `/usage` in JSON print mode. This fallback starts no model turn;
it is labeled **Antigravity CLI**, and automatic updates are disabled for the
short-lived query. It cannot refresh an inactive identified CLI account. The
verified CLI contract and official references are recorded in
[the Antigravity parity ledger](parity/c02b-antigravity-cursor.md#cli-account-selection-investigation-2026-10-22).

Quota observations and complete per-call usage are persisted in Agent HUD's own
SQLite ledger. The five picker measures use separate facts: the current share
used, the current cycle's sampled burn, output tokens per active reply span,
observed cycle consumption, and the model list-price estimate. Reply spans never
bridge logs or idle gaps over 120 seconds, and a reply rate needs at least 3
calls taking 5 seconds in all; fewer leave it unknown. Currencies remain separate.

Source-reported counts take precedence over inferred token consumption. Unknown
cycle boundaries, model mappings or price entries stay explicitly unavailable.
An initial burn series reports its actual sampling progress rather than a rate.
Until it spans enough time, a window with a known start and length forecasts
from the cycle so far on a straight line (used ÷ time since it started), once
3% of the cycle has gone by; that forecast is labeled as estimated from the
whole window and never raises an alert. Such a window also carries a budget:
the share a steady pace through the cycle would have used by now, and how many
points over or under it the use is (more than 2 either way is ahead or under).
The price catalog is a port of the original app's snapshot; this migration does
not assert that every vendor's current list price has been independently refreshed.

The Statistics window charts the readings kept for each shown window over its
selected range, at most 30 days (`quota_history`): one row per window on a
fixed 0–100% axis in the Used/Remaining choice, thinned by Core to a point or
two per bin. A line never crosses a reset (a fall of 5 points in the used share,
or the current cycle's start) or a stretch with no readings over 20 minutes in
which the share moved; an unchanged share within one window length is joined.

Each provider with subscription tiers lists them in its folder
(`agents/src/<agent>/plans.rs`, gathered by `agents::plans`): the vendor's
current name, the monthly list price in US dollars (monthly billing, before
tax, checked on `plans::CHECKED`), the usage multiple of the base paid tier
where the vendor states one, and the spellings its reader reports. Seat-priced
tiers, quotes and names that could be more than one tier (a bare Claude `max`
since the October 2026 rename) are not listed and stay unpriced. Each quota
reading keeps the plan it was taken under (`quota_sample.plan`: the tier's id,
else the reported plan; none for readings before the column existed).

The Statistics window's **Plan value** (`plan_value`) shows each signed-in
account of such a provider: its tier and list price, the provider's calls on
this Mac over the last 30 days at API list prices (calls through a relay or a
coding plan left out), and that as a multiple of the monthly price, marked "≥"
while logs are still read, when the ledger begins inside the 30 days, or when
some calls are unpriced or priced in another currency. Several accounts of one
provider share its calls, so none of them gets a value. Advice appears only
from readings taken under the current tier, for tiers with a stated multiple:
a higher tier when the longest window (a day or more) reached 95% in at least
2 of its last 4 completed cycles, the cheapest that would have held the busiest
to 80%; a lower one when at least 2 such cycles all show their peak (read
within 1/24 of the window before the reset) and every window's busiest use
would have fitted a cheaper tier at 80%.

Existing original history is read without launching or modifying the original
app. Quota samples require exact provider/account/window identities. Transcript
facts require matching source paths, parser codecs, file signatures and completed
retained-history state; business event IDs stay stable for incremental updates.
The original retained history is not described as all-time coverage. Ineligible
files keep their bounded incremental replay and an explicit coverage state.

## Relays

Agents can send their calls through a relay (中转站). The relay reader (`agents::relays`) finds them where Claude Code, Codex and DeepSeek Harness keep the address and key: Claude Code's `ANTHROPIC_BASE_URL` in the `env` of its `settings.json` or `settings.local.json` (in `CLAUDE_CONFIG_DIR`, else `~/.claude`; or Agent HUD's own environment), Codex's `[model_providers.<id>]` `base_url` in `config.toml`, Harness's `llm-deepseek.baseURL` in `settings.yaml` (or `DEEPSEEK_BASE_URL`) with the key it sends there; official and vendor hosts are not relays. A relay is one row per address, whichever clients name it, and is read once with one key (one added by hand, else the first client's). Settings → Relays lists every relay with who uses it, its balance, its switch and its type; an agent's page lists only the relays it uses, with a link to that page.

Who uses a relay is seen, not said: the clients configured with it, then every agent whose calls in the last 30 days were recorded under one of its routes (its host, the Codex providers that point at it, and `openrouter` for `openrouter.ai`).

Nothing is sent to a relay found in a client's configuration until it is switched on (`relayReading`, a list of hosts). Then the key the client already sends that host is sent to the same host, over HTTPS, and nowhere else; Agent HUD keeps no copy of it. A few common templates are tried in turn, each on the relay's own address:

- OpenRouter (`openrouter.ai`): `/api/v1/credits` (credits left), else `/api/v1/key` (the key's remaining limit).
- New API (one-api / new-api), told by its public `/api/status`: `/api/usage/token/` for the account's balance when the site lets the key read it, else the key's remaining quota, in the money the site shows; and its public `/api/pricing`, whose model and group ratios price the calls recorded under that relay's route.
- Sub2API: `/v1/usage`.

A balance becomes one billing row per relay (`vendor` `relay`, `relay` the host, `consumerRoutes` its routes). The snapshot shows it once under each agent that uses it, as that agent's balance (`id` `relay:<agent>:<host>`), and under none when no agent does. A relay's price list prices the calls recorded under any of its routes. A relay no template fits shows a notice and a "Choose type" button. The owner then picks one of the templates for that address, or describes its balance endpoint (custom: a GET path on the relay or an HTTPS address on its host, the JSON path of the balance, a divisor, USD or CNY, and optionally the header the key goes in). The choice is kept by host (`set_relay_kind`), so every client that uses the relay reads it the same way; choosing "find out" forgets it.

A relay's price list may name models its own way. A call's model is matched by its exact name, then without a date, then without a `vendor/` prefix and with dots read as dashes, as long as every such match has the same price.

A relay can also be added by hand on the Relays page, for any agent: its HTTPS address, its key, and a name if the owner wants one (else the relay's own name or host is shown). It belongs to no agent; who uses it is seen as above. It is kept with the other API credentials, and the chosen types, in the private `account-credentials/relays.json`, read without a separate switch, and the key is never shown back.

## Prices

Settings → Prices lists every model the selected logs used in the last 30 days, with its tokens, its price per million tokens and where that comes from. A price is looked for in this order: the owner's own price for the model on that route, then on every route (`customPrices`); free on this computer (a named model server, or a local host the owner calls one in `localKinds`, or by default a local host whose model has no list price); the relay's own list; the vendor's list times the owner's multiplier for the relay (`relayMultipliers`); the vendor's list through its own API or plan. A change prices every log again.

