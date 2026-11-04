# Claude Desktop Cloud metadata (proposal)

This adds remote Project-thread metadata on top of the local-worker support
already on main. The authentication and polling approach remains a proposal for
maintainer review; it does not establish support by Anthropic.

## Collection and privacy

On macOS, the existing account worker reads Claude Desktop's `sessionKey` and
`lastActiveOrg` cookies through the existing OS cookie reader. It sends read-only
requests to unpublished `https://claude.ai/v1/code/...` endpoints, with redirects
disabled. Credentials are not saved by this collector or included in diagnostics.
The collector does not implement Cloud reads on Windows or Linux.

A pass is due 30 seconds after the previous pass ends. Requests use the shared
collection gate, releasing it between requests; provider reads never overlap.
Each network pass has a 25-second deadline, 10-second request timeout, 8 MiB
response cap, bounded pagination, and per-pass environment/decision limits.

Only thread identity, names, timestamps, execution type and state reach the
in-memory session projection. Decision responses may contain conversation data:
the typed reader discards ordinary message bodies and retains only decision
identities, answer state and option counts. Nothing is written to a Cloud cache.
Disabling Claude or detecting a changed/ended sign-in clears remote cards.

## Identity, state and presentation

- Project/thread IDs determine identity; titles do not. Current binding metadata
  distinguishes a local executor from a hosted coordinator. Ambiguous execution
  is unknown.
- Running, waiting, completed, idle and failed states come from remote metadata;
  pending structured decisions can distinguish waiting from idle. Failed reads
  or metadata older than 90 seconds display unknown.
- Remote and local records for the same thread share one row. Local worker usage
  remains counted once and labeled local-only; Cloud total usage is unavailable.
- Rows keep main's detail-first interaction. The detail header uses main's
  validated, bundle-targeted native Project/thread route. Local navigation and
  main's visible data-version re-index are unchanged.

## Validation boundaries

Synthetic tests cover execution changes, decision parsing, freshness, account
invalidation, scheduler fairness/cancellation and preserved local-worker usage.
`desktop_cloud_live` is ignored by default: it needs a signed-in Desktop and
Keychain permission and prints aggregate metadata only. Never commit real
thread identifiers, names, conversation bodies or credentials.

The unpublished endpoints, background use of a sign-in cookie, request cadence
and response to rate limiting remain adoption questions. Browser challenges
and non-JSON responses are treated as failures; the collector does not bypass
them. Earlier personal builds were exercised against live Cloud threads, but
that evidence must not be presented as native acceptance of a new commit.
