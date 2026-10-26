# Macroscope connections

This crate implements host-owned remote MCP connection setup, verification, and local
usage suggestions (ADR 0001 §2). It does not observe the desktop or start a browser.
The app must supply signals only from an authorized active recording or deliberately
typed task text, open the returned authorization URL, and adapt its own vault to
`oauth::TokenStore`. No Muse credentials, account methods, or shared login stores are used.

## Catalog and verification

`contracts/connectors.json` is embedded with `include_str!`; its schema is
`macroscope.connectors/v1`, version 1. URLs are editable through `set_url`.
Gmail and Linear are marked `verified-docs`: this describes documentation evidence,
not successful live authentication. Gmail is a Developer Preview and requires the
user's Google Cloud client ID/secret and the two fully qualified Gmail scopes.
Google's documented web-client examples use registered hosted redirects; compatibility
of a user's client with this crate's random-port loopback redirect needs a live check.

Google Calendar, Google Drive, Slack, Notion, GitHub, and Jira are **unverified** for
the supplied endpoint/registration combination. Their source links are references,
not claims of a completed provider test. Custom's unverified flag means verification
is not applicable until the user supplies and tests a server. Custom supports DCR or
supplied preregistered credentials. Unverified connectors have no invented scopes.

Primary references:
- [Gmail configuration](https://developers.google.com/workspace/gmail/api/guides/configure-mcp-server)
- [Linear MCP](https://linear.app/docs/mcp) and [connector setup](https://linear.app/docs/connect-mcp-servers)
- [MCP 2025-06-18 authorization](https://modelcontextprotocol.io/specification/2025-06-18/basic/authorization)
- [Streamable HTTP](https://modelcontextprotocol.io/specification/2025-06-18/basic/transports)

## Storage and integration

- `TokenStore` has synchronous `get/set/delete` byte operations. The app adapts its
  app-owned Keychain/vault; `MemoryTokenStore` is for tests and ephemeral use.
  `mcp-token:<connector_id>` contains one secret JSON record: access/refresh tokens,
  expiry, resource URL, discovery metadata, and OAuth client credentials. Writing
  one record avoids replacing a working refresh client during a failed reconnect.
  No token or client secret is sent to the state store, webview, or generated code.
  Secret types and pending handles redact Debug; errors omit remote bodies/URLs.
- `StateStore` contains only connection status, URL, tool count, and timestamps.
  `MemoryStateStore` and `JsonFileStateStore` are supplied. The latter writes a
  private sibling file, flushes it, renames it atomically, and syncs the directory.
  The caller chooses an app-owned path and creates its parent directory. Statuses
  are snapshots; call `test` to recheck them. Interrupted Connecting states reset
  on load. Changing the URL deletes authorization and invalidates pending handles.
- The callback binds only `127.0.0.1` on an OS-selected port. It runs immediately,
  accepts one callback, checks state, expires after five minutes, and returns a
  fixed, script-free page with a restrictive CSP. Dropping a handle aborts it;
  UI cancellation should also call `disconnect` to reset persisted Connecting.
  An owned pending handle can be completed only once and by its originating service.
- Access tokens are bound to the exact configured MCP URL. Expiry within 30 seconds
  triggers refresh on `test`. Disconnect deletes the record first and attempts
  authenticated revocation best effort; remote failure cannot retain local tokens.
  HTTP redirects are disabled to prevent credential forwarding. All remote URLs
  require HTTPS, with HTTP exceptions only for `127.0.0.1` and `localhost` local servers.

## Detection privacy

`detect`/`DetectionLedger::observe` consume foreground bundle IDs, window titles,
browser hosts, or task text in memory and discard the signal. App names alone are
not identity evidence; Apple Mail is not mapped to Gmail. Hosts match exactly or
at a wildcard subdomain boundary; titles use case-insensitive literal suffix/contains
rules; task keywords have Unicode word boundaries and Medium confidence.

Returned detections contain only connector ID, signal kind, confidence, and rule ID.
The serializable ledger keeps connector IDs, signal kinds, counts, first/last seen
milliseconds, dismissed IDs, and snooze deadlines. It never keeps raw titles, emails,
text, hosts, bundle IDs, or rule matches. The app owns any ledger persistence and its
retention/deletion policy. Suggestions exclude connected/connecting, dismissed, and
currently snoozed connectors and rank by total count with a stable ID tie-break.

## MCP authority and limits

Connect/test perform only `initialize`, `notifications/initialized`, and paginated
`tools/list`. Protocols 2025-06-18 and 2025-03-26 are supported, including JSON and
incrementally parsed SSE responses. Each response is capped at 1 MiB and each request
at 30 seconds; pagination is bounded to 100 pages/10,000 tools. Tool descriptions
are stripped of control characters and truncated to 1,000 Unicode characters.
Invalid/control-bearing tool names are rejected rather than renamed. All returned
strings, schemas, and annotations remain untrusted data; escape them in the app's
renderer and never interpolate them into instructions.

`call_tool` is `pub(crate)` for host diagnostics and requires a unique tool name,
explicit readOnlyHint, and no destructiveHint. These hints are server claims, not
permission grants. The service never calls tools as part of setup/testing. Generated
code needs a later `mcp.call` broker capability, manifest entry, grant, and staged
approval; this crate provides no generated-code entry point or runtime authority.

## Verification

`cargo test -p macroscope-connections` runs an in-process axum OAuth/MCP provider,
including a test HTTP client playing the browser; it needs no real account. Tests
cover DCR and preregistered clients, PKCE/state, refresh, JSON/SSE, session/version
headers, pagination, size caps, callback expiry, disconnect, URL policy, secret
redaction, atomic state persistence, detection privacy, and suggestion filtering.
