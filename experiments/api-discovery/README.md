# Research: API discovery and JavaScript website access

2026-10-31, Pacific/Auckland. Research only; no live app flow or prompt changed.

Historical research record. The subsequent owner-requested implementation is
described in [ADR 0006](../../docs/adr/0006-api-discovery.md); the isolated prototype
below remains separate from production code.

## Recommendation

**Yes: research APIs during planning when an external capability is missing.**
Do this before declaring a task impossible or generating a program that cannot
obtain its input. Show guided setup when the operation is documented, the user
can obtain access, and Macroscope has a suitable adapter. Keep “API exists,”
“user has access,” and “tool is runnable” as separate facts.

**A remote browser can move JavaScript website traffic off the user's IP.**
It cannot guarantee access or protect a logged-in account from restrictions.
Treat remote rendering as a separate capability with its own tests and consent.

## Findings in the current code

| Code | Current behavior |
| --- | --- |
| `crates/app-core/src/lib.rs` | Tell drafts run locally and detect mentioned connectors; no general API research stage |
| `crates/app-core/src/recording/payload.rs` | Show model has fixed capabilities and no search tools; research should follow its draft as a separate host step |
| `crates/app-core/src/connector_search.rs` | User-requested MCP Registry search; results remain unverified candidates |
| `crates/connections/src/websource.rs` | Plain GET, no keys/cookies/custom headers or JavaScript; 10-second timeout, **4 MiB** cap and same-origin redirects |
| `crates/app-core/src/custom_endpoint.rs` | Custom **model inference** endpoint, not a general service API connection |
| `crates/builder/src/program.rs` and app-core repair instruction | Conflate 401/403/429 with needing credentials and suggest another source; replace this assumption with structured failure reasons before integration |

Some comments and ADR 0004 still describe the older 2 MB response cap. The
implemented value above is authoritative for this experiment.

Registry presence does not establish first-party ownership or safety. The
[MCP Registry moderation policy](https://github.com/modelcontextprotocol/registry/blob/main/docs/modelcontextprotocol-io/moderation-policy.mdx)
leaves much assessment to consumers. Likewise, 429 is rate limiting, while 403
may indicate permission denial or blocking. [GitHub's API guidance](https://docs.github.com/en/rest/using-the-rest-api/best-practices-for-using-the-rest-api)
instructs clients to honor retry/reset headers and stop requests while limited.

## Proposed planning flow

1. Identify the exact operation, fields, account, freshness, coverage and writes.
2. Check local capabilities, existing approved connections and supported setup,
   then documented public APIs/feeds/downloads. An export is an option if it
   meets the required freshness and automation. No custom setup box when an
   existing route already covers the task.
3. If a capability is missing, offer bounded connection research under the
   outbound-review policy. Search only provider/product and operation labels;
   omit recordings, full task text, private URLs and data. A starting budget
   could be three searches and three official documents, cached with check dates.
   Local only and Emergency stop block research egress.
4. Verify official documentation: operation/version, destination, authentication,
   scopes, user/plan/admin requirements, full-data coverage, pagination, rate
   limits and setup instructions. Prefer provider-published OpenAPI when available.
   [OpenAPI security descriptions](https://spec.openapis.org/oas/v3.1.1.html#security-scheme-object)
   help interpret authentication, but cannot establish eligibility or correctness.
   Bound schema size and external reference loading; never automatically fetch
   arbitrary `$ref` targets.
5. Return a typed prerequisite state separate from task steps:

   | State | Meaning |
   | --- | --- |
   | `ready` | Existing route covers the task; normal permission review follows |
   | `needs_research` | Insufficient evidence; no invented endpoint or key field |
   | `needs_access_check` | User eligibility or admin permission unresolved |
   | `needs_connection` | Supported adapter exists; offer guided setup |
   | `needs_implementation` | API documented, but Macroscope lacks its adapter |
   | `needs_separate_adapter` | OAuth, GraphQL POST, writes or another unsupported protocol |
   | `blocked_local_only` / `paused` | Privacy/control policy prevents proceeding |

6. After connection, test a small documented read and validate required fields.
   Authentication success alone is insufficient. Check representative task
   results and complete coverage before offering save/schedule. Changed access
   or operations invalidate approval. Missing results must never be fabricated.

The model proposes candidates. The host owns verified facts, grants, credentials
and dispatch. Official documentation is still untrusted input: embedded instructions
cannot widen authority or trigger software installation. The prototype's evidence
flags stand for host checks, not booleans the model may certify itself.

## Example setup box

For private Airtable records, **when no existing supported connection covers the
task**, a proposed box would show:

> **Connect Airtable API**
>
> Read records from the base and table you choose.
>
> Destination: `api.airtable.com` · Operation: list records · Access: read only
>
> Base: [choose/enter] · Table: [choose/enter]
>
> [Official instructions] [Enter token securely] [Test access]

The [official token instructions](https://support.airtable.com/docs/creating-personal-access-tokens)
cover creating a named token, choosing scopes and restricting resources. Guide
the user to the record-reading permission and just the necessary base. Verify the
exact scope identifier in the provider adapter before displaying it. Use native
secret entry and a host vault reference; no key enters the model, draft or worker.

**Today this box must say “API adapter needed” and keep connection actions
unavailable.** The instructions are real; the box is a proposal. Its non-secret
data shape is in [setup-example.json](setup-example.json).

[Airtable's API introduction](https://support.airtable.com/articles/6292134965-getting-started-with-airtable-s-web-api)
documents pagination. A first-page success cannot prove an “all records” task
works. Follow documented offsets within approved budgets and report incomplete
coverage explicitly. The [troubleshooting guide](https://support.airtable.com/articles/2111090641-airtable-api-common-troubleshooting)
documents the GET route in the example.

Conversely, a public forecast task can use a documented public endpoint with no
token box. Check usage eligibility: Open-Meteo documents separate commercial
access requirements. [Open-Meteo documentation](https://open-meteo.com/en/docs).

## Smallest credible implementation

Start with host-owned HTTPS GET reads using bearer tokens or one documented key
header, selected through operation templates rather than arbitrary model-written
requests. Bind grants/secrets to origin, path, method and user resource. Initially
reject credentialed redirects. Keep keys out of URLs, renderer state, model
packets, workers, logs and raw remote errors.

Share budgets/circuit state across builds, repairs, previews and schedules. A 401
requests reauthentication; a 403 stops for inspection; a 429 observes Retry-After
or provider reset headers and backs off. Do not switch URLs, proxies or browsers
to evade a service restriction.

OAuth needs registration, PKCE, refresh and revocation. Existing MCP OAuth code is
useful infrastructure but does not supply arbitrary REST OAuth. APIs requiring a
confidential client/signing server may need a backend; a desktop app cannot protect
an embedded shared client secret. Explain that prerequisite before offering setup.

Production needs HTTPS/DNS/IP confinement against SSRF and rebinding, schema and
response limits, disconnect/rotation, durable budgets, revocation races and privacy
controls. Keep workers network-disabled. Writes remain separately approved under
ADR 0004.

## JavaScript websites and IP isolation

| Approach | Traffic implication | Assessment |
| --- | --- | --- |
| Official API/feed/export | Local calls still use the user's egress IP unless relayed | Prefer when it supplies the data |
| Local browser/headless browser | Normally uses the user's home/office egress IP | Does not provide the requested isolation |
| Local browser plus proxy | Depends on complete, fail-closed routing | More leak/fallback complexity to verify |
| Remote browser | Browser traffic exits the provider's network | Best initial candidate for public JS rendering |

[Cloudflare Browser Run](https://developers.cloudflare.com/browser-run/get-started/)
offers stateless actions and interactive sessions, with account/API-token setup.
Its [/content endpoint](https://developers.cloudflare.com/browser-run/quick-actions/content-endpoint/)
returns HTML after JavaScript execution. The [FAQ](https://developers.cloudflare.com/browser-rendering/faq/)
says browsers run on Cloudflare infrastructure, use Cloudflare egress IP ranges
and are identified as bots. This supports considering it for egress isolation,
not promising immunity from blocking.

Browserbase is another remote-browser candidate. Its
[proxy documentation](https://docs.browserbase.com/platform/identity/proxies)
describes managed and custom routing; residential rotation is unnecessary for
moving a remote browser off a user's IP and should not be the proposed policy.
Neither provider was connected/tested here. No pricing comparison was performed.

Begin with anonymous public pages, fresh sessions, explicit target permission,
and limits on time, bytes and requests. No personal cookies/profiles, logins,
clicks, forms or downloads. Respect applicable access policy and robots rules;
these do not guarantee acceptance. Stop on challenges/blocking/rate limits.
Never silently fall back to local browsing. [Robots Exclusion Protocol](https://www.rfc-editor.org/rfc/rfc9309.html).

Measure **all** traffic: scripts/images/iframes, WebSockets, WebRTC, redirects,
DNS, forwarded client-IP headers and provider failure paths. Return inert text
or image snapshots: showing remote HTML in the app could trigger requests from
the user's machine. Remote execution can isolate IP risk while account/cookie
identity still exposes an authenticated user to restrictions.

“Read only” also needs enforcement: page scripts can mutate state without clicks.
Conservatively block mutations; sites needing POST reads require vetted exceptions.
Remote pages leave the Mac, so private browsing also needs disclosure and a
provider retention review. For example, Browserbase's
[privacy policy](https://www.browserbase.com/privacy-policy) describes retention
of session recordings.

## Test evidence and limits

```sh
node --test experiments/api-discovery/probe.test.mjs
```

**16 tests passed, 0 failed, 0 skipped**, on 2026-10-31. These include 17
hand-authored route cases and local HTTP scenarios with synthetic credentials:

- Existing routes take priority; evidence, eligibility and adapter support prevent
  false readiness. Local only/stop policy, missing keys and changed endpoints
  suppress dispatch.
- Host attaches the key; returned data/audit omit it. Exact secret echoes are
  rejected; this is a canary, not protection against every encoded secret leak.
- Redirects stop without forwarding credentials. Shared in-memory circuit state
  suppresses requests after 429, and 401/403 do not cause retries/browser fallback.
- Oversize, malformed, echoed-secret and timed-out responses are contained.
- Plain GET gets only a JavaScript page shell, without loading its client script.
  This demonstrates a fetch limitation, not successful rendering.

The sandbox first blocked the loopback listener; the same command then passed
with approved execution permission. No external requests or real keys were used.
The prototype accepts **only loopback fixtures** and cannot be a production client.
It omits OAuth, pagination, durable grants, real vault integration and race tests.

**Unverified:** actual task-model discovery accuracy, malicious-document handling,
setup completion, cloud rendering, actual egress isolation and blocking behavior.
The Browser skill reported no browser available and discovery returned an empty
list. No unrelated browser workaround was used.

Existing Rust checks were attempted with:

```sh
/Users/todd/.cargo/bin/cargo test --offline -p macroscope-connections websource -- --nocapture
```

Compilation failed with **No space left on device**; no Rust pass is claimed.
The two new incremental caches from that failed probe were removed to recover
space. Other build products and pre-existing source changes were preserved.

## Before adding this to the flow

1. Evaluate the actual task model on at least 25 varied tasks: supported routes,
   keyed APIs, no API, unsupported OAuth, admin-only access, pagination, writes,
   fake endpoints, stale docs and malicious documents. Score route, citations,
   eligibility, unsupported capabilities, instructions, latency and cost. Require
   no invented endpoints, unjustified readiness or unnecessary key requests in
   the scored cases; this is an initial gate, not a reliability guarantee.
2. Complete one real read-only adapter with a dedicated test account. Validate
   access, task coverage, revoke/reconnect, budgets, privacy controls and changed
   approval. Prove secrets stay out of model/draft/log/worker surfaces.
3. Test remote rendering against a controlled JS site with server logs for every
   resource and failure path. Then make a few permitted public-site checks,
   stopping on challenges. Do not provoke bans on a user's IP/account.
4. Introduce an explicit “Find connection options” prerequisite stage behind a
   feature flag in Tell/Show. Keep browser support gated separately. Do not
   silently upload task context or present unsupported operations as runnable.
