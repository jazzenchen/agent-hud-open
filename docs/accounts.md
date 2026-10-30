# Account usage ownership

`platform::accounts` reads accounts independently of transcript ingestion. Each
provider has one non-overlapping background job. Network and child-process waits
do not hold the Runtime mutex. A completed reading updates the shared cache and
wakes the application collector; every native window receives that projection.

The implementation follows the original Swift account readers:

| Provider     | Existing source                                     | Read operation                                                       |
| ------------ | --------------------------------------------------- | -------------------------------------------------------------------- |
| Claude       | Installed Claude Code and its current account       | SDK `get_usage`, hooks disabled, `skip_behaviors`, no prompt         |
| Codex        | Installed Codex, its current home and selected additional homes | `initialize`, `account/rateLimits/read`, `account/read`, no turn |
| Pi / ChatGPT | Existing, unexpired Pi OAuth home                   | Official ChatGPT usage endpoint; no OAuth flow or credential rewrite |
| Cursor       | Read-only Cursor account SQLite database            | Current usage-summary endpoint and billing-cycle reset               |
| DeepSeek     | Harness settings and versioned credential reference | Official API balance endpoint and response currency                  |
| Antigravity  | Connected CLI status-line reports and all running authenticated language servers | Identified quota callbacks, serial loopback `GetUserStatus`, anonymous `/usage` fallback |

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

Quota observations and complete per-call usage are persisted in Next's own
SQLite ledger. The five picker measures use separate facts: the current share
used, the current cycle's sampled burn, output tokens per active reply span,
observed cycle consumption, and the model list-price estimate. Reply spans never
bridge logs or idle gaps over 120 seconds. Currencies remain separate.

Source-reported counts take precedence over inferred token consumption. Unknown
cycle boundaries, model mappings or price entries stay explicitly unavailable.
An initial burn series reports its actual sampling progress rather than a rate.
The price catalog is a port of the original app's snapshot; this migration does
not assert that every vendor's current list price has been independently refreshed.

Existing original history is read without launching or modifying the original
app. Quota samples require exact provider/account/window identities. Transcript
facts require matching source paths, parser codecs, file signatures and completed
retained-history state; business event IDs stay stable for incremental updates.
The original retained history is not described as all-time coverage. Ineligible
files keep their bounded incremental replay and an explicit coverage state.

## Relays

Agents can send their calls through a relay (中转站). The relay reader (`agents::relays`) finds them where Claude Code and Codex keep the address and key: Claude Code's `ANTHROPIC_BASE_URL` in `~/.claude/settings.json` `env` (or Agent HUD's own environment), Codex's `[model_providers.<id>]` `base_url` in `config.toml`; official and vendor hosts are not relays. A relay is one row per address, whichever clients name it, and is read once with one key (one added by hand, else the first client's). Settings → Relays lists every relay with who uses it, its balance, its switch and its type; an agent's page lists only the relays it uses, with a link to that page.

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

