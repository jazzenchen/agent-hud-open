# Local network protocol

With **Share with paired phones** on (Settings → Local Network), the app serves what it reads to paired devices on the same network: the iPhone app, or a client of your own. This page covers the four calls such a client needs: pairing, the snapshot, changes and events. The server is `src/crates/lan` (`server.rs`, `cache.rs`, `http.rs`).

Sharing is off until it is turned on in Settings. There is no command-line flag, environment variable or supported settings key for it. While it is off, the app listens on no port.

## Connection

Everything goes over HTTPS to the `endpoint` in the invitation (`https://<address>:<port>`). The certificate is the app's own, self-signed. A client pins it: `certificateSHA256` is the lowercase hex SHA-256 of the certificate's DER bytes. `alternateEndpoints`, when present, lists other addresses of the same app.

The app only listens on TCP. It creates no firewall rules. On Windows, the system asks once whether to allow the app, for the network types ticked there.

## Pairing

Under **Pair a device**, **Create invitation** shows a QR code and **Copy link** copies the same invitation as a link:

```
agenthud://pair?payload=<base64url JSON, no padding>
```

The JSON:

| Field                | Meaning                                           |
| -------------------- | ------------------------------------------------- |
| `schemaVersion`      | `1`                                               |
| `deviceID`           | The app's device id                               |
| `deviceName`         | The computer's name                               |
| `endpoint`           | `https://<address>:<port>`                        |
| `alternateEndpoints` | Optional: other addresses                         |
| `certificateSHA256`  | The certificate to pin                            |
| `token`              | The invitation token                              |
| `expiresAtMs`        | When the invitation expires, in Unix milliseconds |

An invitation works once and expires after 5 minutes. A new invitation replaces the previous one.

Pair with:

```
POST /v1/pair
{"token": "<invitation token>", "clientID": "<a stable id for your client>"}
```

- `200`: `{"deviceID", "deviceName", "endpoint", "certificateSHA256", "token"}`, plus `alternateEndpoints` when there are any. This `token` is the client's own; keep it.
- `400`: the body is not that JSON.
- `410`: the invitation was used, replaced or has expired.

Every other call sends `Authorization: Bearer <token>`. A missing, unknown or revoked token gets `401`. **Revoke paired devices** in Settings withdraws every token.

## Snapshot

```
GET /v1/snapshot
```

```json
{
  "schemaVersion": 1,
  "deviceID": "…",
  "deviceName": "…",
  "revision": "…",
  "generatedAtMs": 0,
  "records": [],
  "cursor": { "baseVersion": "2026-11-01T08:00:00.000Z", "changeSequence": 3 }
}
```

- `revision` is a hash of the content. It is also the `ETag`: send it back as `If-None-Match` and an unchanged snapshot answers `304` with no body.
- `cursor` is where this snapshot stands, for `/v1/changes` and `/v1/events`.
- `503` means the app has not published anything yet, for example right after launch.
- With `Accept-Encoding: gzip`, bodies of 1 KiB or more come gzipped.

## Changes

```
GET /v1/changes?since=<baseVersion>,<changeSequence>
```

`since` is the snapshot's `cursor` written as `baseVersion,changeSequence`, for example `since=2026-11-01T08:00:00.000Z,3`. Percent-encoding the value is fine. It is not the `revision`.

- `200`: `{"schemaVersion", "deviceID", "deviceName", "fromCursor", "toCursor", "revision", "generatedAtMs", "records", "deletedIDs"}`. `records` holds every record written after `since`, `deletedIDs` the ids removed since then. Use `toCursor` for the next call.
- `400`: `since` is missing or not in that form. `since=0`, for example, gets a `400`.
- `410`: the app cannot go on from that cursor. Fetch `/v1/snapshot` again and continue from its `cursor`.

History is kept in memory for about 24 hours. After the app restarts, every earlier cursor gets a `410`.

## Events

```
GET /v1/events
```

A server-sent event stream (`text/event-stream`) that says when something new was published.

- `event: revision`, with `data: {"revision", "cursor"}`: there is a new version. Fetch `/v1/snapshot`, or `/v1/changes` from your cursor.
- With `X-AgentHUD-Stream: changes`, the stream sends the changes themselves instead: `event: changes`, its `id` the new cursor and its `data` a change set as `/v1/changes` returns it. Give a `since=` cursor, as for `/v1/changes`, to start from there; without one, the first event is a `revision` notice and changes follow from it. When a change set is larger than 256 KiB, a `revision` notice comes instead, so fetch it from `/v1/changes`, gzipped. `event: reset` means the stream cannot go on from where it stands; fetch the snapshot again.
- With nothing new, a `revision` notice or a heartbeat comment arrives every 30 seconds.

The stream ends when sharing is turned off or the client's token is revoked.

## Link

```
GET /v1/link
Upgrade: websocket
```

One long connection in place of the requests above: a WebSocket, opened with the same `Authorization: Bearer` header. A client that keeps a link needs no other request. The one-request routes stay as they are for clients from before the link.

Every message is a JSON text frame.

- A request: `{"id", "method", "path", "cursor"?, "etag"?, "scope"?, "session"?, "body"?}`. Its answer: `{"id", "status", "etag"?, "body"?}`, where `status` and `body` are what the same route answers on its own. `cursor` is a cursor object as a snapshot gives it; `etag` a revision.
- `GET /v1/snapshot`, `GET /v1/changes` and `GET /v1/events` are the routes above. `GET /v1/events` answers `200` and begins a feed: events arrive as `{"id", "event", "data", "revision"?, "cursor"?}` under the request's `id`, `event` one of `revision`, `changes` and `reset`. Asking again replaces the feed; `DELETE /v1/events` ends it.
- `"scope": "list"` leaves token usage and conversation turns out of a snapshot, a change set and a feed. With `"session": {"provider", "sessionID"}` that one session's turns are included. `GET /v1/conversation` with a `session` answers that session's turns, in a snapshot's shape.
- `"scope": "light"` leaves no record out, token usage included, but sends every conversation turn without its words: its payload's `text` and `items` are dropped, and everything else about the record stays as it is. With `"session": {"provider", "sessionID"}` that one session's turns come whole. `etag`, `revision` and `cursor` are still those of the whole content, so a client that reads this way checks that its position follows on and does not recompute the revision. `deletedIDs` are not filtered. An app from before this scope answers with the whole content.
- Any other route goes to the app's own, such as `POST /v1/prompt`, with `body` as its JSON body.
- `{"ping": n}` is answered `{"pong": n}`. Send one every 25 seconds: a link the app hears nothing on for 90 seconds is closed.

The link ends when sharing is turned off or the client's token is revoked. A message may hold up to a megabyte.
