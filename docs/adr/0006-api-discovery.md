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
model calls and two documentation reads. Research has a fixed 1,000 model-call
monthly limit, shared across checks, as a guard against runaway use (100 proved too low: creating
tools every day reached it). The output layout suggestion and the one source search a build may make
when a source cannot be used are part of making a tool and do not count toward it.

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

## Addendum 2026-10-31: extra key headers, sibling documentation domains, plain-language setup

- **Extra key headers.** A proposal and a saved connection may name up to two further key
  headers beside the main credential (an app id next to an access token). Only the names
  are stored in settings; each value is entered in its own native dialog and kept in the
  vault under `api-credential:<id>:<n>`. Names follow the existing header rules and may not
  repeat. All values are removed on disconnect and are covered by the exact-secret-echo check.
  The worker still cannot add or change headers.
- **Documentation on a sibling domain.** The evidence rule still needs a retrieved document
  that contains the endpoint origin, and a searched proposal still has to cite a returned
  source. The provider-domain test now also accepts a documentation host with the same
  registrable name under a different suffix (`api.akahu.io` with `developers.akahu.nz`).
  IP addresses and names under four characters never match. This remains an evidence check,
  not proof of legitimacy: a page the attacker controls could already satisfy the old rule.
- **Cursor pagination.** `cursor.next`, `next_cursor` and `nextPageToken` strings now fail a
  read like the other pagination indicators, so a first page is never mistaken for the whole
  set. Following cursors is still out of scope; an endpoint whose answer fits one page, or a
  bounded date range, works.
- **Plain-language setup.** Setup shows what will happen in three steps, how many keys will
  be asked for, and tucks the address and header fields into a collapsed "Technical details"
  (open for a manual custom API). Connections shows Custom APIs as their own visible section.

## Addendum 2026-10-31 (2): discovery for problems that name no service, free personal access first

- **Problem-first search.** The model may return a generic `searchQuery` (the problem, the kind of
  access and the region, never task data). When it names no service, that phrase is searched with
  the route's existing search (hosted `web_search`, or the person's Brave key, now returning eight
  candidates instead of five). Only when there is neither a service nor a phrase does research stop.
- **Region.** The system locale's country (`en_NZ` gives "NZ") is added to the reviewed payload, or
  `null` when unknown, so the model can prefer services that work where the person lives and say
  when the answer depends on a country it does not know.
- **Free personal access first.** The model is told to prefer, in order: connected tools, supported
  connections, free public APIs, free personal-access APIs (a personal app, token or free plan the
  person creates for their own data), and only then paid, partner-only or accreditation-gated APIs,
  naming the alternatives it passed over. Each proposal carries a `cost` read only from retrieved
  documentation (`public`, `free_personal`, `free_tier`, `paid_or_approval`, `unknown`) and the
  review shows it in plain words; `unknown` says the cost was not confirmed.
- **Not verified.** Tests prove the plumbing: the problem phrase is searched, the preference text
  reaches the model, and cost is carried to the person. Whether a given model actually finds and
  ranks the right API for a new problem needs a live evaluation set; that has not been run.

## Addendum 2026-10-31 (3): a program that says its data is missing is not repaired

The builder's rule is that a program throws `new Error("one plain sentence")` rather than invent a
missing file, account or connection. The correction loop used to treat that as a bug and ask the
model for up to three fixes. Now, when a correction fails with the same deliberate `Error` sentence
as the attempt before it, the build stops with the new failure kind `missing_data` and shows that
sentence. Only a literal sentence from the program counts: engine faults (`TypeError`,
`ReferenceError`) and messages built from data (redacted to "…") keep every correction round.
The failure screen offers "Check how to get this data" (back to the plan and its access check),
"Use a file instead" and "Change the request", and no "Try again", which could not help.

## Addendum 2026-10-31 (4): a chatty or wrapped answer does not fail the access check

A live access check on the Meta API route failed with "output does not match the requested schema".
Two causes are plausible and both are closed: the access schema capped list sizes and string lengths,
so a longer answer was rejected whole; and a reply that had just searched the web can wrap its JSON
in a sentence or code fence. The access schema now keeps only types, enums and required fields; every
length and count cap is applied by trimming after the answer is parsed. When the call used search, the
object between the first `{` and the last `}` is parsed if the whole reply is not JSON, and is still
validated against the schema. The prompt states the limits so the model aims for them. The remaining
generic failure now says the answer wasn't in the needed form and to check again. The exact live
cause was not observed; this is inferred from the code and covered by a test that fails without it.
