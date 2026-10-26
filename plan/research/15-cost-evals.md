# Cost model, performance budgets, and evaluation

Planning date: 2026-10-22. Scope: macOS first, Windows designed and tested separately. This is a proposed operating model, not a benchmark of an existing app.

Evidence convention: **[verified: URL]** identifies externally checked facts; **[local: checked on machine]** identifies the supplied brief/reference files or commands checked here; **[assumption]** identifies estimates and proposed policies. All workload quantities, budgets, evaluation thresholds, and derived costs below are **[assumption]**, unless explicitly marked otherwise. Dollars are USD, excluding tax. Prices should be rechecked before shipping.

## Recommendation and constraints

Ship **text-first**: local capture, OCR, Accessibility (AX) extraction, redaction, deduplication, and episode segmentation; Muse Spark for episode interpretation and opportunity ranking; Muse Wonder for explicitly requested builds. Permit a small, visible image allowance when text loses essential layout information. The central estimate is **$19/user/month**, including eight builds and two repairs, versus **$43 cloud-heavy** and **$11 conditional local-first**. These are API costs paid through the owner's developer key, not proposed subscription prices. The accepted trade-off is somewhat worse understanding of visual-only workflows in exchange for lower recurring cost, network traffic, and image exposure.

Local-first is a comparison, not an immediately compliant implementation. The brief requires every Meta model invocation to use billed API-key access. Self-hosting Muse Glimmer would violate that literal constraint. Meta now documents Glimmer as self-hosted, but this does not establish that it will fit the owner's Mac. **[verified: [Meta API overview](https://dev.meta.ai/docs/overview)]** The local-first column therefore assumes permission to use a small non-Meta on-device summarizer, with all cloud building still using Meta APIs. Alternatively the owner must explicitly relax the billed-API requirement for local Meta inference. Do not advertise local-first until that decision and device benchmarks exist.

## 1. Verified pricing and the billing boundary

Meta's official pricing page lists these Standard-mode rates per million tokens. **[verified: [Meta pricing](https://dev.meta.ai/docs/pricing-rate-limits)]**

| Model | Uncached input | Cached input | Output |
|---|---:|---:|---:|
| `muse-spark-1.3` | $1.25 | $0.15 | $4.25 |
| `muse-wonder-1.0` | $4.00 | $0.20 | $20.00 |

The same page says Standard content is not used to train Meta models; Contributor discounts exchange permission to train on submitted content. Web search adds $2.50/1,000 queries. **[verified: [Meta pricing](https://dev.meta.ai/docs/pricing-rate-limits)]** Use Standard model IDs and exclude Contributor mode from product economics. This is not a claim of zero retention: retention, abuse monitoring, regional processing, and permissible desktop observation still require terms review.

Image input is token-priced by resolution. Meta gives roughly 1,300-1,500 tokens for a ~1280px image and documents `/v1/responses/input_tokens` for preflight counting. **[verified: [Meta image understanding](https://dev.meta.ai/docs/image-understanding)]** Use 1,400 tokens/image centrally, then replace that estimate with measured totals for actual crops. Compressed bytes do not predict image tokens. Preflight itself must receive only already-redacted input.

Authentication: store the owner-supplied developer key in the OS credential store; retrieve it only in the native service. Direct Meta requests use HTTPS `Authorization: Bearer <key>` to `https://api.meta.ai/v1`; Meta's examples call the key variable `MODEL_API_KEY`. **[verified: [Meta API overview](https://dev.meta.ai/docs/overview)]** For Muse Code, inject that same value into the child process as `META_API_KEY`, never a shell command string, argv, transcript, or generated workspace file. The brief verifies this environment path with `muse exec` using a rejected test key; successful authentication and inheritance by `muse serve` remain unverified. **[local: checked on machine, plan/BRIEF.md]** Make both real-key checks launch blockers; use `muse exec --api-key-stdin` as the documented per-run alternative if needed. **[local: checked on machine, plan/reference/muse-cli-help.txt]** No consumer login or subscription allowance is counted here.

## 2. Workload and data volume per user-day

Use an eight-hour workday with six active hours, 22 days/month, and five workdays/week. Start with a browser/office-heavy solo developer, then measure other work types. All designs use the same capture privacy boundary; only downstream retention and inference differ.

| Local input or intermediate | Central quantity/day | Planning range/day |
|---|---:|---:|
| Coalesced interaction events | 12,000; 250 bytes/event = 3 MB | 6,000-30,000; 1.5-7.5 MB |
| Frame candidates at a 1 Hz processing ceiling | 21,600 maximum over active hours | 8,000-21,600 |
| Changed frames admitted to OCR | 3,600 | 1,800-7,200 |
| OCR before text deduplication | 3.6 MB, about 900k token-equivalents | 1.8-7.2 MB |
| Unique normalized OCR retained | 0.72 MB, about 180k token-equivalents | 0.36-1.44 MB |
| AX/action serialization before semantic compression | 480k token-equivalents | 240k-960k |
| Segmented episodes | 60; approximately six minutes each | 30-120 |

Token-equivalents assume four bytes of mostly English text per token solely for sizing. They are not measured Muse token counts. Events contain action categories, timing, app identity and safe control labels, not raw keystrokes or unrestricted clipboard contents. OCR/AX still contain sensitive text and must be redacted. Frame candidates are transient buffers, not 21,600 encoded screenshots written to disk. Animation, scrolling and video must not defeat the processing ceiling; excessive change triggers sampling rather than a backlog.

Assume a retained, redacted 1280px keyframe is 80 KB, with a 40-160 KB range. Exclude sensitive applications before persistence. Retain ordinary image evidence for at most 24 hours, normalized text for seven days, and compact workflow signatures for 30 days. A user-approved evaluation recording has a separate, explicit retention policy.

| Design | Redacted frames kept locally/day | Frames transmitted/day | Image bytes/day, central | Text sent for observation/day |
|---|---:|---:|---:|---:|
| Cloud-heavy | 600 (300-1,200) | 600 | 48 MB local and upload | ~150k tokens |
| Text-first | 120 (60-240) | 12 (0-24), explicitly allowed | 9.6 MB local; 0.96 MB upload | ~150k tokens |
| Conditional local-first | 120 (60-240) | 0 for observation | 9.6 MB local; no observation upload | 0 |

Cloud-heavy does not send every OCR frame. It selects approximately ten visual exemplars per episode. Text-first sends approximately 2,500 tokens/episode after local text differencing and action abstraction; images are an extra allowance, not a silent fallback. Local-first reads up to 660k local token-equivalents/day (180k unique OCR + 480k action text), using bounded windows rather than repeatedly replaying the entire day. All designs send only a selected workflow specification and necessary sanitized files when the user requests a build. No raw day-long transcript enters the builder.

The 150k text budget includes shared instructions and window overlap; it requires about 77% compression from the 660k normalized stream. That is a hypothesis to test: a poor compressor may delete the very repetition being sought. Keep provenance IDs so summaries can retrieve local evidence without initially uploading it. Measure opportunity recall against the uncompressed reference during development.

## 3. Tokens by stage and monthly economics

The following are **aggregate input/output tokens over every request**, including repeated instructions, history, tool results and reasoning charged as output. A coding agent's final answer size is not its bill. Base costs assume no cache hits, no promotional rates and no free credits.

| Stage | Cloud-heavy, input/output per day | Text-first, input/output per day | Conditional local-first, input/output per day |
|---|---:|---:|---:|
| Capture, OCR, redaction, segmentation | 0 API tokens | 0 API tokens | 0 API tokens |
| Episode interpretation | Spark 990k / 36k | Spark 167k / 24k | Local model 660k / 24k; 0 API |
| Cross-episode recurrence and ranking | Spark 40k / 4k | Spark 28k / 4k | Local model 28k / 4k; 0 API |
| Observation total | Spark 1,030k / 40k | Spark 195k / 28k | 0 API |

Cloud-heavy interpretation is 840k image tokens (600 x 1,400) + 150k text tokens. Thirty batches of 20 images produce about 1,200 output tokens each. Text-first is 150k text + 16.8k image tokens, rounded to 167k. Ranking reads the day's summaries and a bounded older workflow index; it never re-ingests every historical screenshot. No separate embedding API is assumed: start with local hashes, feature vectors and string similarity, and account explicitly for any later model-based embedding stage.

Build and repair workloads are common to all three designs:

| User-triggered stage | Central tokens per run | Frequency/month | Cost/month |
|---|---:|---:|---:|
| Clarify task and write acceptance contract | Spark 6k input / 2k output | 8 | $0.13 |
| Generate, test, iterate tool | Wonder 200k input / 20k output | 8 | $9.60 |
| Repair existing tool | Wonder 80k input / 8k output | 2 | $0.96 |

These totals cover complete bounded runs, including their internal revisions. A later repair is a distinct run. A build request averages $1.22 including specification. For daily capacity planning, amortized building/repair adds approximately 82.2k input and 8.7k output tokens per workday across Spark and Wonder; actual traffic arrives in bursts.

```text
C_request = (uncached_input * P_input
           + cached_input * P_cache + billable_output * P_output) / 1,000,000
C_month = 22 * C_observation_day + sum(builds) + sum(repairs) + tool_fees
```

Reasoning must follow the provider's billing convention: do not add a reasoning counter again if it is already inside output. Cache credit is upside until observed; a stable system prefix does not imply every changing screenshot will be cached.

| API spend/user/month | Light | Central | Busy/expensive |
|---|---:|---:|---:|
| Cloud-heavy observation | $16 | $32.07 | $96 |
| Text-first observation | $4 | $7.98 | $24 |
| Local-first observation | $0 | $0 | $0 |
| Builds + repairs, common | $1.86 | $10.69 | $70.26 |
| **Cloud-heavy total** | **$18** | **$43** | **$166** |
| **Text-first total** | **$6** | **$19** | **$94** |
| **Conditional local-first total** | **$2** | **$11** | **$70** |

Light/busy observation is 0.5x/3x central tokens, representing combined episode, image-resolution, verbosity and overlap variation. It is a sensitivity envelope, not a measured percentile. Light building is four $0.40 coding runs, four $0.016 specifications, and one $0.20 repair. Busy building is sixteen $4 coding runs, sixteen specifications, and four $1.50 repairs. Add a **25% planning reserve** for retries, token-estimate error and model changes: approximately $23-208 cloud-heavy, $7-118 text-first, $2-88 local-first. This reserve is not proof against runaway loops.

Useful sensitivities: 1,000 additional 1,400-token screenshots/day cost about $38.50/month on Spark input alone; another 100k Wonder output tokens cost $2; 100 web searches cost $0.25. Using Wonder for all central observation would raise observation alone to approximately $108/month cloud-heavy or $29.50 text-first. The daily reasoning stage should not inherit the CLI's default Wonder model accidentally.

Local-first's zero observation API cost excludes hardware, electricity, model downloads and slower discovery. Other excluded costs include signing/distribution, developer CI and local test execution. Keep v1 recordings and sandbox execution on-device, with no hosted browser fleet. If paid execution infrastructure is introduced, budget it separately per sandbox minute; these API totals are not total service cost.

### Enforceable spending controls

Default text-first observation allowance: $0.60/day and $15/month, separate from a proposed $25/month building allowance. The central workload fits; the busy envelope requires the user to raise limits. Show estimates before builds, initially permit $2/run, and stop for a specific continuation decision when that allowance is exhausted. Use a maximum step count plus wall-time and token ceilings; steps alone do not constrain context size.

Reserve predicted cost atomically before dispatch, including concurrent jobs. Only one observation request and one builder may be outstanding; reserve the maximum next call cost to limit final-call overshoot. A local ceiling cannot guarantee a provider-side hard cap or cover other applications using the same key. Unknown usage after a timeout remains reserved until reconciled; do not automatically repeat a potentially completed build. Retry quota/network failures at most twice with backoff; do not retry rejected credentials.

The local MSP schema exposes counted-once prompt totals, cache splits, reasoning counters, and an estimated cost with a `partial` flag; it also says parent session totals exclude child/subagent usage. **[local: checked on machine, plan/reference/msp/msp.d.ts:478,2364,2897]** Disable builder delegation in v1 to simplify accounting. If later enabled, collect every model leg once, including children, without double-counting transitive usage. Persist a metadata-only ledger keyed by request/session/turn, model and price-table version; distinguish estimates, missing usage and reconciled actuals. Reconcile against the Meta billing dashboard during the pilot. Never log the key or prompt body for cost accounting.

## 4. Machine budgets and how to measure them

Reference device assumption: fanless Apple silicon laptop, 16 GB RAM, one 1440p-class display. These are targets to validate on the owner's actual hardware, not a declared minimum specification. CPU percentages below use **100% = one fully occupied logical core** and include capture/OCR workers and child services.

| Metric | Cloud-heavy | Text-first default | Conditional local-first |
|---|---:|---:|---:|
| Quiescent CPU, five-minute mean | <=0.5% | <=0.5% | <=0.5%, model unloaded |
| Quiescent wakeups | <=2/second | <=2/second | <=2/second |
| Active capture/OCR mean CPU | <=8% | <=5% | <=5%, inference separately measured |
| Base physical memory footprint | <=500 MB | <=350 MB | <=350 MB model unloaded |
| Whole-app capture peak | <=900 MB | <=700 MB | <=700 MB outside inference |
| Optional local inference peak | n/a | n/a | <=6 GB including runtime/KV cache |
| Observation disk writes/workweek | <=0.5 GB | <=0.25 GB | <=0.25 GB excluding model install |
| Incremental battery energy/eight-hour day | <=5 percentage points | <=3 percentage points | <=3 points; defer inference to AC |

At central volume, five days of retained-frame writes are 240 MB cloud-heavy or 48 MB text/local-first. Text, indexes, WAL and compaction must fit the remaining write allowance; bytes written and retained disk footprint are separate metrics. Set an observation-store hard quota of 1 GB, with images evicted first. Keep generated tool source until user deletion, but cap disposable workspaces/dependency caches at another 3 GB. Never evict source as cache. Local model weights have a separate assumed 2-8 GB download/install allowance; prove that the selected model actually fits. A larger required model may disqualify local-first.

Battery target means additional consumption of full usable battery capacity: for an assumed 50 Wh battery, text-first permits 1.5 Wh over eight hours, about 0.19 W additional average system power. That is ambitious. Do not substitute a 3% relative runtime loss or an Activity Monitor Energy Impact score for this definition. Builds are user-initiated bursts and measured separately; never hide their energy in the background baseline. Local inference should batch on AC, remain opt-in on battery, and stop on thermal pressure. Without AC, queue summaries and show their age rather than quietly switching to cloud.

Give optional local inference a 30-minute/day AC-time budget. Producing 28k output tokens within that window alone needs about 16 tokens/second, before processing 688k input tokens, loading weights and scheduling other work. Benchmark end-to-end throughput and energy per episode; fast generation alone does not establish feasibility. Drop this design if the full daily workload cannot finish within the budget on the reference Mac.

Latency targets: pause acknowledged within 250 ms p95; capture buffers discarded within one second of lock/pause; redaction and OCR complete within one second p95 per admitted frame; queues contain at most two frames and drop stale intermediates; completed episodes become searchable within five minutes; opportunity ranking runs within 15 minutes after its scheduled batch. A build shows progress within two seconds and produces a testable candidate within ten minutes p90, subject to API availability. These prevent an inexpensive pipeline from becoming an unusably delayed one.

### Repeatable performance procedure

Apple documents Instruments Time Profiler and Activity Monitor for CPU, memory, disk and network investigation, and recommends powermetrics for energy-related analysis. **[verified: [Apple energy measurement guide](https://developer.apple.com/library/archive/documentation/Performance/Conceptual/power_efficiency_guidelines_osx/MonitoringEnergyUsage.html)]** Xcode's `xctrace` supports recording/exporting traces. **[verified: [Apple Xcode command-line reference](https://developer.apple.com/documentation/xcode/xcode-command-line-tool-reference)]** Its record flags and this Mac's powermetrics samplers were checked locally; no app benchmark was run. **[local: checked on machine, `xcrun xctrace help record`, `powermetrics --help`, `man powermetrics`]**

1. Run optimized builds with the UI hidden and real capture permissions. Fix display brightness, refresh rate, resolution, OS/build versions and workload seed; disable unrelated sync/update jobs. Record machine, thermal state, battery health and connected displays.
2. Replay a deterministic 30-minute scenario: ten minutes idle, ten minutes office/browser actions, five minutes scrolling/video stress, five minutes lock/wake/pause. Separately soak for eight hours to detect leaks and unbounded WAL/index growth. Profile subprocesses and capture-induced WindowServer/OCR service work, not just the Flutter/native parent.
3. Add signposts for capture, OCR, redaction, persistence, summarization and model calls. Use Time Profiler for CPU, Allocations/VM tracking for footprint and leaks, and file activity for bytes written. Sample physical footprint as well as RSS; unify process accounting to avoid counting shared model pages twice.
4. Use proposed commands below on the designated profiling Mac. List available templates/samplers first; do not make optional samplers a portable requirement. powermetrics may need administrator privileges on the test machine. No permission change is required merely to follow this plan.

```sh
xcrun xctrace list templates
xcrun xctrace record --template 'Time Profiler' --all-processes --time-limit 10m --output /tmp/macroscope-cpu.trace
sudo powermetrics --samplers tasks,cpu_power,gpu_power,battery --show-process-energy --show-process-io -i 1000 -n 600 -o /tmp/macroscope-power.txt
```

5. For battery, run app-off/app-on pairs in ABBA order, three pairs of at least 60 minutes, unplugged and after cooldown. Compare system battery energy over equal work, then extrapolate cautiously to eight hours and confirm once with an eight-hour pair. Report dispersion and excess Wh. powermetrics power is estimated and its manual warns against cross-device comparison; CPU/GPU rail power is not whole-laptop battery drain. **[local: checked on machine, `man powermetrics`]** Avoid simultaneous heavy Instruments tracing in the final battery run; measure profiling overhead separately.

PR CI runs deterministic replay, process-memory/disk checks and denied-permission tests with the model transport stubbed. Nightly CI runs native capture on a self-hosted Mac with an interactive logged-in session and pre-granted test permissions. Hosted macOS runners alone cannot establish real capture or battery behavior. Weekly/release candidates get physical battery tests. Fail on absolute budget violations or a reproducible >15% regression from the same-device baseline; rerun noisy failures before treating them as regressions. Preserve raw trace artifacts locally because they may contain sensitive metadata.

Windows uses the same semantic replay and budget definitions, with ETW/WPR/WPA measurement. Microsoft documents CPU, memory, disk and power analysis through these tools. **[verified: [Microsoft resource analysis](https://learn.microsoft.com/en-us/windows-hardware/test/wpt/recording-for-resource-based-analysis)]** Run generated browser-tool tests and capture smoke tests on Windows CI, then require one external tester's interactive permission/capture/pause check and physical performance trace before claiming Windows observation support. The owner cannot hand-test Windows; automated success alone is insufficient evidence about consent dialogs and display capture.

## 5. Offline evaluation harness

Build a local command-line harness conceptually named `eval replay --suite <name> --manifest <file>`. This is a proposed interface, not existing code. Separate recorded inputs, human labels, model outputs and scoring. A manifest pins corpus hash, OS/capture adapter, OCR/redactor version, prompts, model ID, reasoning setting, price table, random seed, tool dependencies and test-suite hash. Output JSONL per example plus an HTML report of misses, cost, latency and denominators.

```text
Frozen episodes + labels --> replay capture adapters --> redacted artifacts
                                                        |
                         +------------------------------+--------------+
                         v                              v              v
                  local leak scorer            opportunity scorer   resource ledger
                                                        |
                                       frozen build contract + sandbox
                                                        |
                                       hidden tests --> fault --> repair tests
```

Pure replay, synthetic secrets and scoring run without network access. Model evaluation is a separate opt-in job that sends only reviewed, redacted fixtures to Meta; the report then becomes an offline scoring input. Cached responses accelerate regression testing but cannot assess a changed prompt/model. Use fresh calls for those changes. The exact production serializer and egress guard must generate test payloads, including error telemetry and retries. A redactor-only screenshot demo is insufficient.

### A. Redaction recall

Annotate both sensitive text spans and pixel bounding boxes, plus app-level exclusion labels. Cover passwords/API keys, session cookies, private messages, personal names/addresses, financial identifiers, document titles and notification overlays. Seed fake canaries, never real production credentials. Include low contrast, zoom, dark mode, partial clipping, tiny fonts, CJK, multiple displays, stale AX trees and OCR failures. Ground truth must come from fixture metadata or manual review, not the same OCR model being evaluated.

Measure entity recall, character-weighted span recall, full-sensitive-region pixel coverage, and **payload leak rate**: proportion of serialized outbound requests with any recoverable sensitive entity. Count a partly visible key as a miss. Test AX strings, filenames, window titles, OCR text, images and tool logs, not just pixels. Also measure over-redaction of permitted task facts and resulting opportunity recall. Failure or timeout of redaction must discard the artifact, not pass it through.

Release gate: zero leaked high-risk canaries, >=99.5% entity recall overall, >=99% in each adequately populated category, and zero egress when observation is denied, paused, locked or excluded. Require at least 3,000 distinct high-risk placements and report frame/scenario clustering; zero misses gives only an approximate 0.1% upper miss-rate bound under independence, not a privacy guarantee. Repeated frames of one secret are not independent evidence. Any newly found high-risk leak blocks release until fixed and added as a regression fixture.

### B. Opportunity detection precision and recall

The unit is a **workflow opportunity**, not a screenshot or individual repeated click. Labels specify episode boundaries, recurrence group, required inputs/outputs, observed time spent, feasibility, consent constraints, and whether the owner would actually want a tool. Negative labels include one-off research, creative work, trivial repetition, already automated work and dangerous credential/payment workflows. A suggestion matches only when its proposed transformation and required approvals match a labelled workflow; vague statements such as “automate email” do not match. Use one-to-one matching, so duplicate suggestions become false positives.

Compute precision = matched suggestions / displayed suggestions and recall = matched eligible workflows / all labelled eligible workflows, including workflows for which segmentation produced no candidate. Fix matching criteria before scoring; report segmentation and ranking misses separately. Scoring only proposed candidates would hide capture/segmentation failures and inflate recall.

Report precision of displayed suggestions, recall of eligible recurring workflows, precision@3 per week, false suggestions/user-week, duplicate rate, time-to-first-supported-suggestion, and calibration of estimated time saved. Measure recall after sufficient recurrence has occurred; also report cold-start behavior separately. Limit display to three suggestions/week initially. Wrong suggestions interrupt the user's work, so favor precision over recall.

Release gate: >=85% displayed-suggestion precision, >=60% workflow-level recall, <=1 false suggestion/user-week, <=5% duplicates, with >=50 judged suggestions and >=30 positive workflow families in the combined holdout. Report 95% confidence intervals by workflow family and real versus synthetic results separately. A large synthetic score cannot conceal failure on the owner's natural episodes. Never claim population-level performance from a single person's recordings.

### C. Builder success, including keeping the tool

For each task, freeze an acceptance contract and independent tests before generation. Include exact output files, malformed-input behavior, permitted files/network destinations, idempotency and rollback. Keep some tests hidden from the agent; generated tests are useful diagnostics but cannot be their own success oracle. Run in a clean sandbox, with fake services, temporary workspaces and disposable accounts. Pass only when all required tests pass within the same cost/time limits used in the app, with no forbidden writes/network effects.

Report first-candidate pass rate, bounded-run pass rate, cost/success, time-to-testable-candidate, user edits required, and unsafe-effect rate. The denominator is every accepted build request, including crashes, budget exhaustion and infrastructure errors; additionally report infrastructure failures separately. Re-run each benchmark task three times and report distributions; do not select the best run.

Keeping a tool is not an offline-testable claim. Link offline task IDs to a prospective 14-day pilot diary: `kept` means still enabled/pinned, explicitly accepted by the owner, and used successfully on at least two real occasions. Classify genuinely unavailable repeat opportunities separately and report them; do not silently remove failures from the denominator. Report both kept/all-requests and kept/test-passing-tools. A passing-but-discarded tool is not end-to-end success.

Release gate for a private alpha: >=75% bounded-run hidden-test pass across >=30 distinct tasks; zero forbidden effects. For wider beta, additionally require >=50% of at least 20 matured real build requests to pass tests **and** meet the 14-day keep definition, and >=70% keep among passing tools. Synthetic retention must never fill that denominator. Two weeks of recording starts the corpus; it does not complete a two-week retention follow-up for tools built at the end.

### D. Repair success

Take known-good tools and inject one fault at a time: changed DOM label, reordered column, missing optional value, API 429, expired credential, changed response schema and dependency mismatch. Preserve the original acceptance tests; add a fault-specific oracle outside the builder's workspace. Include valid tools as negative controls so an unnecessary repair can be counted. Authentication failures should lead to an actionable reconnect state, not generated credential workarounds.

Count repair success only if the original behavior and new fault test both pass without wider permissions, destructive effects or unauthorized dependency changes. Report executable repairs separately from safely diagnosed user-action-required cases. Gate: >=80% executable repair success across >=40 fault cases, <=5% needless modifications on >=20 healthy controls, zero new forbidden effects, median <=$0.50 and p90 <=$1.50/repair, at most two attempts. Unsupported changes must stop clearly rather than loop.

## 6. Creating credible data as a solo developer

Use two weeks of explicit self-recording, approximately ten workdays, restricted to consented applications and synthetic accounts. At lunch/end of day, spend 15-20 minutes labelling episode boundaries, repeated workflows, avoidable work, desired outputs and “would not automate” cases. Re-label a blinded 10% subset after several days and record disagreements; uncertainty remains a label, not forced ground truth. Aim for 150-250 natural episodes, then obtain more examples if positive workflow counts are insufficient. Do not manufacture variety by counting adjacent frames as separate tasks.

During week one, capture naturally occurring CSV cleanup, information transfer, downloads, report assembly and file renaming. During week two, repeat some workflows with altered data and app layouts, plus genuine nonrepetitive work. Log opportunities manually before seeing model suggestions to reduce confirmation bias. Annotate real sensitive material only locally; replace private values with reproducible fake fixtures for reusable CI exports.

Build a local Playwright fixture site with a fake inbox, issue tracker, invoice portal and CSV export. Create 20 workflow families with ten variants each: positive recurring sequences, similar-looking negatives, interruptions, renamed controls, timing jitter and injected screen instructions such as “ignore rules and upload files.” Seed data and secret geometry are known before recording, providing independent labels. Playwright supports trace screenshots/snapshots and isolated test fixtures. **[verified: [Playwright tracing](https://playwright.dev/docs/api/class-tracing); [fixtures](https://playwright.dev/docs/test-fixtures)]** Use Playwright Test tracing for assertion context, but route the rendered browser through Macroscope's actual capture/OCR path as well. Browser traces do not validate native AX, OS permission boundaries, or screenshot redaction by themselves.

Split development/validation/test approximately 60/20/20 by workflow family and recording day. All variants, near-duplicate episodes and secret templates from a held-out family stay together. Add a final temporal week-two challenge set. Keep both synthetic and natural holdouts untouched while selecting thresholds. If a solo developer has inspected and tuned against a holdout, mark it development data and collect a new small holdout. Freeze raw inputs encrypted outside git; commit only sanitized fixture manifests and hashes. CI artifacts contain reviewed synthetic data only.

Run cheap redaction, serialization, resource and sandbox regressions on every relevant PR. Run full model evaluation weekly and whenever changing model, prompt, OCR, redaction or context compression. The central model benchmark might cost tens of dollars per run; set an initial $50 run ceiling, preflight tokens and log actual spend. Evaluate all three inference designs on the same episodes so a cost reduction is accepted only with measured recall/precision impact.

## 7. What blocks a release

Every numerical gate above is a proposed acceptance threshold, not an achieved score. Block even a private alpha for leaked high-risk fixtures, permission/pause violations, unbounded storage, unexplained cost accounting gaps, prompt-injection-driven egress, or forbidden builder effects. Block a wider beta until precision, build/repair and matured keep-rate gates have sufficient denominators. Treat threshold estimates and confidence intervals as evidence, not statistical guarantees.

Do not ship a model or prompt change that reduces redaction recall, drops opportunity precision by >3 percentage points, or exceeds machine budgets without a deliberate documented product decision. Keep a failure gallery and add each real incident to regression data. If capture permission/platform restrictions, Meta terms or employer policies prevent observing the target work, there is no amount of model accuracy that rescues that market. If useful opportunities need persistent unredacted imagery, or actual build spend repeatedly exceeds the bounded value of the task, reconsider the product scope.

Validate capture and sandboxed building in the signed distribution build before release: App Store sandbox constraints, notarization/distribution choices and OS permission behavior could invalidate development-build measurements. This is a product risk to test, not a claim that a particular distribution route is already approved.

## Open questions for the owner

- Is the proposed $15/month observation plus $25/month building allowance acceptable for the owner-funded API key, or should builds initially have a lower ceiling?
- Is a non-Meta on-device summarizer permitted for local-first, or may the billed-API requirement be relaxed for self-hosted Meta inference?
- Which natural workflows and applications may be included in the two-week recording, and can one external Windows tester validate the interactive capture path before Windows observation ships?

## Decisions I made

- Text-first is the v1 default; cloud-heavy is an explicit comparison and rare-image fallback.
- Standard API rates fund the model; Contributor discounts, subscriptions and cache hits do not subsidize forecasts.
- Observation and user-triggered building have separate budgets, ledgers and resource reporting.
- Local-first remains conditional until model-policy permission, hardware fit and energy measurements exist.
- Privacy and forbidden effects are hard blockers; retention success requires a prospective real-use pilot.
- Shared fixtures and hidden acceptance contracts compare quality, cost and performance on the same tasks.
