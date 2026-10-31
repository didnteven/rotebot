# ADR 0006: Research missing API access during planning

Status: accepted for the first implementation, 2026-10-31. Source: owner request
to build API discovery while continuing to use suitable known APIs/connections.

## Behavior

Program plans in both Tell and Show offer **Send and check access**. Preparing
the review is local; this action authorizes the displayed task description and
connection metadata to leave the Mac. File contents, recordings and credentials
are excluded. This initial implementation keeps research explicit, rather than
adding undisclosed network calls to the existing local Check the plan action.

The selected task model first assesses existing connected services and known
documented APIs. A suitable connected service is offered for per-tool selection;
a service with an existing supported setup opens that setup before a custom API
is considered;
a public API needs no credential setup. Research only fills missing access, and
adding a new API preserves other selected connections.

The Meta API route can use provider-hosted `web_search`, capped at three built-in
tool calls. Other model routes can inspect known documentation and optionally use
the user's Brave Search API key for broader research. They never silently switch
to another inference route or borrow its credential. Each check makes at most two
model calls and two documentation reads. Research has a fixed 100 model-call
monthly limit, shared across checks.

API setup requires a retrieved document on the endpoint provider's domain (allowing
the usual `api.` and documentation subdomains), containing the endpoint origin.
Newly searched proposals must cite an actual returned search source. This is an
evidence check, not proof of provider legitimacy, endpoint semantics, account
eligibility, pricing or full-data coverage. The user reviews the proposed destination
and instructions; the subsequent connection and task checks establish more.

Results distinguish local work, existing access, public APIs, required setup,
missing resource details, missing adapters and unresolved research. Unsupported
OAuth/POST/write operations do not receive an operational key-entry form. An
unresolved access report blocks building that plan until access is resolved. The
report is bound to the task kind, goal and selected connections, and becomes
inapplicable when that binding changes.

## Custom API read connections

The Connections screen and supported research results provide the same setup form:
name, complete HTTPS endpoint, authentication mode and documentation URL. Modes are
no authentication, bearer token or a documented API key header. Credentials enter
through a native dialog and live only in the host vault. Configurations are immutable;
replacement endpoints receive a new connection id and require new per-tool grants.
Saving is local. Testing issues one explicit GET and validates JSON before the
connection is available for selection.

Each connection exposes one fixed `read` source tool with empty arguments. It
uses the existing source approval, tool version/grant, sandbox, staging and
scheduling paths. Generated code receives only the response. It cannot override
the endpoint, method or header, and cannot request external actions on this adapter.
Disconnect removes the credential and configuration, invalidating saved tools
that depended on it.

The transport pins public DNS answers and rejects private/reserved destinations,
URL credentials/secret query names, redirects and unsupported headers. It disables
system proxies and automatic retries, has a ten-second HTTP timeout, and caps API
responses at 4 MiB and research documents/search responses at 256 KiB. Test-only
loopback HTTP is available through the existing explicit fixture switch.

Known pagination indicators fail explicitly; this adapter does not crawl pages or
prove arbitrary JSON is complete. Paginated APIs, provider-specific response
validation and more complex authentication need dedicated adapters. Exact secret
echoes are rejected, while remote error bodies and raw transport diagnostics are
discarded. This is not a guarantee against arbitrary encodings of a malicious
server's content.

Per-origin cooldown state is durable and shared by custom API reads and research.
401/403 stop automatic reads; 429 honors Retry-After. Explicit connection tests
can recheck corrected access after any cooldown. Local only and Emergency stop
cancel network work; cancellable HTTP futures acknowledge the existing revocation
barrier even when an enclosing source read is dropped.

Public web-source build failures with HTTP 401, 403 or 429 now stop without the
repair model repeatedly fetching or substituting sources. Builder instructions
distinguish authentication, refusal and rate limits instead of assuming a key
fixes all three.

## Boundaries and validation

Remote/local JavaScript browsing is outside this change. Existing Custom endpoint
settings remain model inference settings. No automatic signup, software install,
OAuth registration or external write is introduced by API research.

Core integration tests exercise production planning, gateway, credential transport,
connection selection and grant paths with local HTTP stand-ins. A synthetic-runtime
test builds/saves/runs an API-backed tool, checks actual returned data, cancels an
in-flight staged read and verifies disconnect prevents further reads. The worker
test verifies integration, not new evidence about the OS sandbox itself.

Frontend tests cover explicit research consent, existing connection reuse, public
APIs without setup, unsupported OAuth/POST, native-only secret entry, Local only,
inert untrusted content and stale responses. Provider-hosted search, Brave results
and model quality were not tested against paid live accounts. No browser was
available for visual verification. Model discovery accuracy and real provider
setup completion therefore remain release-validation work.

Relevant primary documentation:

- [Meta search grounding](https://dev.meta.ai/docs/search-grounding)
- [Meta Responses schema and max_tool_calls](https://dev.meta.ai/docs/api-reference/responses/schemas)
- [Brave API authentication](https://api-dashboard.search.brave.com/documentation/guides/authentication)
- [Brave web search API](https://api-dashboard.search.brave.com/api-reference/web/search/get)
- [Initial research and isolated prototype](../../experiments/api-discovery/README.md)
