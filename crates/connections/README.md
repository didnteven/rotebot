# Macroscope connections

This crate implements host-owned remote MCP connection setup, verification, and local
usage suggestions (ADR 0001 §2). It does not observe the desktop or start a browser.
The app must supply signals only from an authorized active recording or deliberately
typed task text, open the returned authorization URL, and adapt its own vault to
`oauth::TokenStore`. No Muse credentials, account methods, or shared login stores are used.

## Catalog and verification

`contracts/connectors.json` is embedded with `include_str!`; its schema is
`macroscope.connectors/v1`, version 1. URLs are editable through `set_url`.
All eight provider entries are `verified-live`, sourced to **unauthenticated probe
2026-10-27**. This verifies public discovery behavior; authenticated OAuth flows
have not been verified. Custom remains unverified until configured and tested.

| Connector | MCP URL | Anonymous initialize | PRM resource | Authorization server | Registration |
| --- | --- | --- | --- | --- | --- |
| Gmail | https://gmailmcp.googleapis.com/mcp/v1 | 200; tools/list also open | Origin 404 HTML; path PRM uses MCP URL | https://accounts.google.com | Preregistered |
| Google Calendar | https://calendarmcp.googleapis.com/mcp/v1 | 200 | Origin assumed absent; path PRM uses MCP URL | https://accounts.google.com | Preregistered |
| Google Drive | https://drivemcp.googleapis.com/mcp/v1 | 200 | Origin absent; path PRM uses MCP URL | https://accounts.google.com | Preregistered |
| Slack | https://mcp.slack.com/mcp | 401, origin PRM challenge | https://mcp.slack.com | https://mcp.slack.com | Preregistered |
| Notion | https://mcp.notion.com/mcp | 401, path PRM, scope `default` | https://mcp.notion.com/mcp | https://mcp.notion.com | Dynamic |
| GitHub | https://api.githubcopilot.com/mcp/ | 401, path PRM `/mcp/` | https://api.githubcopilot.com/mcp/ | https://github.com/login/oauth | Preregistered |
| Linear | https://mcp.linear.app/mcp | 401, path PRM, scope `read write` | https://mcp.linear.app | https://mcp.linear.app | Dynamic |
| Jira | https://mcp.atlassian.com/v1/mcp | 401, no PRM challenge | Absent, origin/path 404 | https://mcp.atlassian.com | Dynamic |

Discovery tries the challenge's `resource_metadata`, then path-suffixed and origin
`/.well-known/oauth-protected-resource` URLs. Absent PRM falls back to the catalog's
`auth.authorization_server`, then the MCP origin. Authorization server discovery
uses RFC 8414, falling back to OIDC configuration on 404/405. HTML 404s are treated
as absent before parsing. A PRM resource must share the MCP origin and prefix the
MCP URL. That resource is sent in authorization, code exchange, and refresh requests;
without PRM the MCP URL is used. Scopes prefer the catalog, then the challenge's
`scope`, then PRM `scopes_supported`.

The initial 2026-10-27 probe reported Google's absent origin PRM. The supplementary
live run found path-suffixed PRM at `/.well-known/oauth-protected-resource/mcp/v1`
for all three Google servers. It advertises `https://accounts.google.com/`, while
AS metadata advertises `https://accounts.google.com`; discovery accepts equivalent
origin URLs but still rejects different issuer paths. Google also currently serves
RFC 8414 metadata; OIDC remains the fallback.

Google discovery provides: authorize `https://accounts.google.com/o/oauth2/v2/auth`,
token `https://oauth2.googleapis.com/token`, revoke `https://oauth2.googleapis.com/revoke`.
Gmail requests `https://www.googleapis.com/auth/gmail.readonly` and
`https://www.googleapis.com/auth/gmail.compose`; Calendar requests
`https://www.googleapis.com/auth/calendar.readonly`; Drive requests
`https://www.googleapis.com/auth/drive.readonly`. All three use `access_type=offline`
and `prompt=consent` for refresh tokens. **UNVERIFIED:** `send_resource_indicator=false`
for Google because its OAuth endpoint is not documented to accept RFC 8707 `resource`.
Google's Developer Preview requires the relevant API and MCP API enabled and a
preregistered desktop OAuth client; `auth.client_setup` supplies UI instructions.
Slack authorizes at `https://slack.com/oauth/v2_user/authorize`; GitHub uses OIDC-style
metadata and a GitHub OAuth app. Jira exposes RFC 8414 at its origin with `/v1/authorize`,
`/v1/token`, and `/v1/register`.

`auth.fixed_redirect_port` defaults to none (random port). Slack and GitHub use
**53682**, so their app registrations can use the exact redirect
`http://127.0.0.1:53682/callback`. If occupied, connecting fails with the port number
and an instruction to close the application using it. Google uses a random loopback
port. All preregistered entries include `auth.client_setup` instructions.

Primary references:
- [Gmail configuration](https://developers.google.com/workspace/gmail/api/guides/configure-mcp-server)
- [Linear MCP](https://linear.app/docs/mcp) and [connector setup](https://linear.app/docs/connect-mcp-servers)
- [MCP 2025-06-18 authorization](https://modelcontextprotocol.io/specification/2025-06-18/basic/authorization)
- [Streamable HTTP](https://modelcontextprotocol.io/specification/2025-06-18/basic/transports)

## Storage and integration

- `TokenStore` has synchronous `get/set/delete` byte operations. The app adapts its
  app-owned Keychain/vault; `MemoryTokenStore` is for tests and ephemeral use.
  `mcp-token:<connector_id>` contains one secret JSON record: access/refresh tokens,
  expiry, configured MCP URL, OAuth resource/indicator preference, discovery metadata,
  and OAuth client credentials. Writing one record avoids replacing a working refresh client during a failed reconnect.
  No token or client secret is sent to the state store, webview, or generated code.
  Secret types and pending handles redact Debug; errors omit remote bodies/URLs.
- `StateStore` contains only connection status, URL, tool count, and timestamps.
  `MemoryStateStore` and `JsonFileStateStore` are supplied. The latter writes a
  private sibling file, flushes it, renames it atomically, and syncs the directory.
  The caller chooses an app-owned path and creates its parent directory. Statuses
  are snapshots; call `test` to recheck them. Interrupted Connecting states reset
  on load. Changing the URL deletes authorization and invalidates pending handles.
- The callback binds only `127.0.0.1` on an OS-selected or configured fixed port.
  It runs immediately, accepts one callback, checks state, expires after five minutes, and returns a
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

`mcp::preview_tools(url)` performs an unauthenticated initialize and paginated tools/list
for capability previews before connecting, preserving `readOnlyHint` and
`destructiveHint` exactly, including absent hints. Protected servers return
`AuthRequired`. Connect verification and `test` send a Bearer token whenever one is
stored, even if the server also serves tools anonymously.

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


Discovery fixtures cover Google catalog AS/OIDC fallback with HTML 404 PRM responses,
Atlassian origin AS/RFC 8414, challenge/path/origin PRM precedence, Linear origin
resource indicators through code exchange and refresh, scope precedence, resource
validation, open previews, Bearer verification, and fixed redirect ports.

The ignored `live_discovery_matches_catalog` test probes all eight providers using
only unauthenticated initialize and metadata GETs. It asserts authorization server
issuers and DCR availability against the catalog; it never sends authorization,
registration, or token requests. Run explicitly with:

```sh
MACROSCOPE_LIVE_MCP=1 cargo test -p macroscope-connections -- --ignored live_discovery
```
