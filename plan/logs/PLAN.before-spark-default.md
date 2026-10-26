# Macroscope master build plan

Research baseline: 22 October 2026; master plan completed and revised against the [technical/UX review](reviews/codex-review.md) on 23 October 2026. Owner: one developer; macOS first; Windows subsequently. Status: proposed implementation plan, with gates that have not yet passed. Review dispositions: [CHANGES](reviews/CHANGES.md).

This document supersedes competing recommendations in the research reports. Read in full: [BRIEF](BRIEF.md) and all fifteen files in [research](research/): 01 product, 02 competition, 03 Meta models, 04 Muse/MSP, 05 alternative builders, 06 macOS capture, 07 Windows capture, 08 privacy/security, 09 patterns, 10 runtime, 11 and 12 architecture, 13 and 14 UX, and 15 cost/evaluations. Their conflicting proposals are resolved below rather than accumulated into one oversized MVP.

Evidence notation applies throughout:

- **VERIFIED-local** means an observation already recorded in the supplied research/probe artifacts; it does not mean this planning pass reran the experiment.
- **VERIFIED-docs** means primary documentation, including the Meta pages rechecked during synthesis. Documentation establishes a supported contract, not tested reliability.
- **UNVERIFIED** marks an implementation dependency, unresolved external claim, or behavior needing a real-key/platform experiment.
- Everything specified as a decision, threshold, estimate, schedule, schema, or wireframe is **proposed**, unless explicitly verified. Dollar values are USD. No benchmark or release gate below is claimed achieved.

## 0. Summary and key decisions

Build a personal desktop utility that turns a deliberately shown or described task into a small, tested tool. Its first useful output is a CSV/text/file transformation that runs deterministically without a model. The first session must deliver value before requesting broad observation. Subsequently, opt-in local discovery identifies recurring work and asks the user to teach or confirm the actual rule. The durable product object is a versioned tool with a behavior contract, proof results, permissions, and run receipts.

The launch user is a technical Mac power user willing to supply a Meta developer API key. The longer-term user is an operations generalist, but bundled billing, enterprise monitoring, arbitrary computer control, and a connector marketplace are separate businesses. Do not build them to get the first ten users.

Implement Tauri 2/React/TypeScript and Rust coordination/policy. Keep Swift macOS and native Rust Windows observers as separately gated tracks. Evaluate Muse MSP over child stdio before selecting the shipping builder; if its admission or confinement gate fails, immediately use direct Meta zero-tool artifact generation. Generate TypeScript against a small effect-broker SDK only after one pinned Deno/macOS boundary is demonstrated. Result, rules, checked examples, and access are the primary review surfaces; transcript/source are advanced details and a terminal is deferred.

Use one app-owned OS-vault key for direct Meta API calls and the child-only `META_API_KEY` lane to Muse. Standard models only. Rejected-key probes establish serve environment ingestion; successful billing, live approvals, and complete usage accounting remain unverified. Probe 21 adds a session-start permission-profile failure, not successful inference or proof of the app-key lane. Do not require a consumer login. Do not touch Muse's shared credential store.

The private MVP is **Tell → one create-only CSV transformation family → independent checks and host-owned comparison → save tool → manual second-input run**. Capture, terminal, rename/move, invoices/reports, general undo, full repair UI, discovery, and browser integration are later increments. Keep their concrete designs below, without making them first-product dependencies. The first two weeks buy evidence, not a complete automation product. Re-estimate after measured integration and support effort; prior 12–16/20–28-week dates are optimistic capacity hypotheses, not delivery commitments.

### Key decisions

Each row records the choice, the rejected recommendation, and the reason in one sentence.

| Topic | Chosen | Rejected option | Why |
|---|---|---|---|
| Initial wedge | Tell and one create-only CSV family; Show experiment separate | Six families plus capture as private-MVP dependencies | A useful second-input run tests value before expanding infrastructure. |
| Discovery scope | Later opt-in local metadata mining with conservative cards | No discovery indefinitely (01), continuous cloud interpretation (03/15) | Discovery preserves the original thesis while local mining bounds exposure and spend. |
| Product structure | Tools/Create first, Activity and Privacy; Builds as task detail | Inactive Discoveries as front door, transcript-first IDE | Add Discoveries only when its evidence gate passes. |
| Task understanding | Editable structured Task Contract | Transcript as authoritative specification | A corrected contract makes business rules and acceptance criteria explicit before generation. |
| Desktop stack | Tauri 2 + React/TS + Rust | Electron, Flutter, SwiftUI plus WinUI | One web interface and a native resident core fit a solo developer's background utility. |
| Mac observer | Swift process in signed bundle | Rust-first Apple framework bindings (12) | Direct Apple APIs reduce capture, TCC, and lifecycle integration work. |
| IPC | Inherited pipes; XPC for Mac observer identity | General UDS/gRPC services (12) | The parent-child topology needs no discoverable network service. |
| Database ownership | Core is sole persistent writer | Observer/engine/shell each own separate writable DB (12) | Central migrations and deletion lineage are simpler at the selected ingest volume. |
| Generated artifact | TypeScript/Deno after runtime gate; finite CSV mapping fallback | General graph DSL, Python scripts, or unconfined code if Deno fails | Preserve expressiveness only when its boundary is demonstrated. |
| Runtime authority | All user-data reads and effects through broker | Direct filesystem reads for generated Deno (10) | Brokered reads prevent code from bypassing previews, limits, and audit policy. |
| Builder integration | MSP evidence spike; direct zero-tool Meta fallback on failed gate | Building dependent UI while admission/confinement is unresolved | MSP exposes typed sessions but has an observed startup blocker. |
| Muse hosting | One active restricted host per posture; one active build globally | Per-tool permanent hosts, network-enabled default host | Reuse limits startup cost while serial builds simplify accounting and workspace isolation. |
| Build shell | Disabled in MVP; host-owned tests | Key-bearing unrestricted agent shell (04/12) | The observed inherited key makes arbitrary shell execution an avoidable credential risk. |
| Terminal | Deferred; later key-free workspace shell, Muse TUI separately gated | Terminal in first private product | Result comparison must work without operating an IDE. |
| Sandbox assertions | Demonstrated enforcement or block capability | Workspace isolation, Job Objects, or static scans as sandbox proof | None independently constrains every filesystem or network effect. |
| Consent | Capture, egress, build tools, and runtime grants separate | One onboarding approval for everything | Recording an action does not authorize a model request or its future execution. |
| Capture defaults (later) | Manual five-minute session; native selected window; browsers excluded initially | Always-on screenshots, title heuristics for private mode | Narrow scope is testable and fails closed when private context cannot be established. |
| Keyboard/clipboard | No keyboard monitoring; clipboard only explicit run input | Rhythm hooks, clipboard HMAC observer (06/09) | The first supported workflows do not justify keylogger-shaped collection. |
| Image/model policy | No image upload by default; reviewed crops to analyzer only | Cloud per-frame vision, raw screenshots to coding agent | Structure and synthetic fixtures suffice for the first buildable workflows. |
| Models | Spark for bounded interpretation; Wonder for requested builds/repairs | Weekly Wonder mining, local Glimmer, Contributor discounts | Rare high-value generation warrants Wonder while API-only Standard use meets the brief. |
| Costs | Sparse discovery; about $11/month baseline build workload | $19 text-every-episode, $43 cloud-heavy, $32 continuous model plan | Most episodes need no paid interpretation before a local candidate exists. |
| Retention | Images 24 h; sanitized events 7 d; aggregates 30 d | Seven-day images, 90-day detailed events, indefinite episodes | Short evidence retention is adequate for teaching and limits the sensitive archive. |
| Repair/trust | Later explicit repair/reapproval; every MVP run previews staged bytes | Full repair UI, automatic promotion in first product | Clean runs do not prove behavior on a new input. |
| MVP integrations | One selected CSV → one new CSV; manual only | Clipboard, moves, invoices, browser acting, arbitrary scripts, runtime LLMs | Create-only output gives the strongest early correctness oracle. |
| Distribution | Developer ID/notarized direct Mac; signed MSIX Windows | Mac App Store, Windows NSIS-first, bundled Muse | Native observation needs direct distribution and MSIX supports the chosen Windows OCR route. |
| Platforms | Apple Silicon macOS 15/26; Windows 11 x64 later | macOS 14/Intel/universal and Windows ARM parity immediately | Supported targets require hardware evidence the owner currently lacks. |
| Billing | Each external tester supplies their own key | Shipping the owner's key, bundled credits in v1 | BYOK avoids a proxy/data-processing service and shared-key abuse. |

## 1. Product: who, what, and measurable value

### 1.1 Persona and positioning

Build for Jordan: an independent developer, technical PM, or agency operator on a personally controlled Mac who repeatedly cleans exports, prepares reports, renames files, and transforms copied text. Jordan can create an API key but wants to stop spending an evening writing a thirty-line script. The private launch promise is: **Describe a repetitive CSV cleanup; keep a converter you can check and run again.** Show and broader tasks extend that promise only after their feature gates.

The primary outcome is removing active work after accounting for review and repair. Avoid a general productivity dashboard or searchable-life archive. The competitor research shows substantial overlap with memory tools, task mining, and natural-language builders; novelty of the three-stage pipeline is not a defensible claim. The working differentiation is a shorter route to a tested, persistent utility with clear authority and portable source. Validate it against describing the same job to an existing coding agent, measuring total explanation, review, second-input use, and maintenance effort.

The later operations-generalist persona needs easier billing and support, neither assumed solved by this MVP. For now, use only data the tester is entitled to process. Workplace or managed-device use requires an explicit policy check; a BYOK design does not settle legal roles or authorize sending employer information.

### 1.2 First ten workflows and their exact boundary

The first ten remain a product backlog and evaluation portfolio. Build only #1 for private MVP; use the others as unsupported controls until independently admitted. Each admitted family needs distinct task instances separated by rule rather than filename variation; thirty tasks in one family must exercise different mappings, formats, and error rules.

| # | Workflow and input | Output and success criterion | Delivery |
|---|---|---|---|
| 1 | Reshape an export CSV for import | New CSV with explicit column order, row count, date/currency rules; original hash unchanged | MVP anchor |
| 2 | Reformat a copied snippet | Previewed JSON/slug/address/text transform; explicit clipboard read/write only after a user gesture | After retained CSV value |
| 3 | Rename and sort exports/screenshots | Frozen old→new list, collisions rejected, checked recovery into chosen root | Later move/undo gate |
| 4 | Combine two reports | Reconciliation CSV and unmatched-row report using user-confirmed stable key | Later family |
| 5 | Weekly issue status from downloaded export | Local Markdown/text draft and CSV; all required issue IDs preserved; user pastes/sends manually | Later family |
| 6 | Build a client invoice draft from hours CSV | Host-rendered escaped HTML draft with decimal totals and explicit rounding; no invoice issuance, PDF engine, or email send | Later family + renderer gate |
| 7 | File invoices/receipts | Known-layout extraction plus proposed filenames; unknown vendor/layout asks for help; no fuzzy model-at-run fallback | M5 after tested PDF recipe |
| 8 | Prepare product photos | Selected images converted/resized with native fixed recipe and deterministic naming; originals preserved | M5 after image recipe |
| 9 | Extract visible web-admin table | Chromium companion, user-selected origin/table, TSV output; no cookies, submission, or everyday-profile CDP | M5 read-only browser expansion |
| 10 | Open an admin record or start workspace | Validated URL template from explicit ID, or app/URL launch list; no login, window driving, or inferred query upload | M5 native open recipe |

Use #1 as the end-to-end spike and the clearly labeled bundled onboarding sample. Teach capture later enriches understanding but cannot discover exact CSV contents or business meanings that were never read. The user supplies representative input and expected output locally. A future browser-export demonstration has explicit content gaps until the extension exists; those gaps do not prove field-level actions.

### 1.3 MVP in/out

The private MVP includes signed Mac installation; a bundled sample and Tell; a plain-language editable task summary backed by the Task Contract; local selected-file fixtures; secure Meta key setup; bounded API-billed generation; host-owned checks/result comparison; immutable source/version integrity; enforced selected-file reads and create-only output; staged-byte approval; save/manual rerun; and receipts. Source and encrypted fixtures need deletion/expiry, but no capture archive is created. Section 8 gates every shipped surface; deferring a surface removes that dependency, never weakens its security requirement.

Keep these out: capture/Show and capture storage, terminal, clipboard/hotkey integration, rename/move, invoice/report families, general undo, full repair UI, ambient mining, spontaneous discovery notifications, browser control/companion, network-enabled tools, runtime LLM nodes, folder auto-runs, schedules, OAuth, native form filling, arbitrary script execution, Python, custom generated UI, billing/accounts, sync, sharing, gallery, teams, and mobile. A failing input can be corrected locally or become a new explicitly reviewed build; do not add a repair system to solve initial scope. Unattended triggers need readiness, deduplication, authority, and recovery tests.

Public discovery, a Chromium companion, and a constrained folder trigger are a second product increment, each separately gated after retained tool value. None is a prerequisite for a useful Mac Tell release. Browser acting, API integrations, native recipes, and schedules need separate admission tests. Mobile can later be an approval/notification companion; it is never a v1 capture target.

### 1.4 Metrics and business boundary

Use **weekly active retained tools** as the north star: an enabled tool with at least one successful real run during the week. A 14-day retained tool additionally requires two real successful uses and explicit user acceptance. Report median estimated net minutes saved per active user alongside it; subtract first-build review, recurring review, repair time, and failed-run recovery. Never equate elapsed foreground time with savings.

Private-beta hypotheses: 50% of installers save a useful tool; with prerequisites available, at least four of five formative users complete a personal second-input run within thirty minutes; p90 candidate generation below ten minutes after rule confirmation; at least 75% bounded-build success; at least 70% of passing tools retained; at least 50% of matured accepted requests become retained tools. Start the product clock at first launch and record install duration separately. Report sample completion separately from a useful personal tool; split working-key/dependency and fresh-account cohorts, but count setup failures and abandonments in the installer denominator. Track explanation/review time, cost per retained tool, repairs/tool/month, and permission violations. Zero known high-risk leakage or unauthorized effects is a release condition, not a marketing guarantee.

Do not implement monetization before retention. Run a free, invite-only BYOK beta. Test an $8/month or annual software license hypothesis in interviews only; no app billing work is authorized by this plan. Deterministic tools must keep running if billing is ever introduced and then lapses. Source and manifest remain exportable; safe standalone execution requires a matching runner and is later, so do not promise every exported source folder runs by itself.

## 2. UX: contract, evidence, proof, and control

### 2.1 Principles and information architecture

Show useful output before requesting permissions; distinguish observation from inference and instruction; present exact scopes before effects; keep interruptions rare; show reversible recovery where available. Treat capture status, tool status, and model/key status as separate facts. A build may be working while observation is paused.

Use a shared React UI with system font, native title bars, normal focus behavior, platform shortcut labels, restrained accent color, and solid labeled redaction bars. Do not attempt a SwiftUI imitation through dozens of custom window effects. All essential actions have keyboard and pointer paths. Support VoiceOver/Narrator labels, large text, reduced motion, visible focus, and a list alternative to dragging steps.

Private-MVP destinations are **Tools** (opens first, with **Create tool / Describe**), **Activity**, and **Privacy**. Build progress is task detail; hide Discoveries and Show until enabled by their gates. Settings holds stable preferences, model/key configuration, and diagnostic versions. Later add Discoveries and advanced Builds as the library grows. A compact native menu handles Stop build/tools, Local-only, Open, and Quit without needing a webview; Pause observation appears with capture. Do not keep hidden editors alive indefinitely. UI wording is “Task summary,” “Changes need checking again,” and “Create one CSV in Reports; leave the source unchanged”; contract/signature/capability terms stay in details.

### 2.2 Onboarding wireframe and permission sequence

The sample is a bundled, prebuilt CSV converter running on synthetic files. Label it as a sample; it proves preview/run behavior without implying a live paid build or personal value. Ask for the key immediately before the first model request. The second wireframe is the deferred Show experiment, hidden in private MVP; it asks for capture rights only when selected.

```text
+------------------------------------------------------------------+
| Macroscope                                      Setup saved      |
| Turn a repetitive task into a tool you can test and keep.         |
|                                                                  |
| [Try a sample]    [Describe a CSV task]                            |
|                                                                  |
| SAMPLE: messy CSV -> clean import                                |
| Input:  date=10/22/26, amount=$1,240.00                            |
| Output: date=2026-10-22, amount=1240.00       [Compare] [Run sample]|
|                                                                  |
| Selected data reaches Meta only after an outbound review.         |
| No observation is needed to make this tool.   [Privacy details]    |
+------------------------------------------------------------------+

+------------------------------------------------------------------+
| Show one task                              maximum 5 minutes     |
| Scope [Choose one allowed native window...]                       |
| Excluded: browsers, messaging, terminals, finance, password tools  |
|                                                                  |
| 1 Accessibility   [Explain and enable] [Use Tell instead]          |
| 2 Screen access   [Choose window / enable] [Skip screenshots]      |
| 3 Preview         [Start a 20-second local check]                  |
|                                                                  |
| [Back]                                 [Start demonstration]     |
+------------------------------------------------------------------+
```

Order: sample/value → choose scope and exclusions → Accessibility for field geometry → Screen Recording/picker → local sanitized preview → explicit recording. No Input Monitoring prompt in MVP. No Apple Events prompt until a future per-target recipe needs it. Persist progress before a possible OS-requested relaunch. Recheck real access after returning from Settings; saved onboarding completion is not a permission grant.

For Tell, the order is sample/Describe → local CSV preview → example-based rule questions → key/dependency check → outbound review → generation → compare → save → second input. Ask “Does 03/04/26 mean March 4 or April 3?” rather than requiring schema editing. State API-account/billing prerequisites before download. If MSP is the selected builder, show Muse missing, unsupported version, download failure, signature/fingerprint mismatch, updater removal, and permission-profile failure as distinct setup states. Provide official installation guidance and Retry checks without shell-pasting secrets, automatic consumer login, or discarding the draft. Offer the already validated direct path when available; never silently change the builder. Direct generation does not require a separate Muse installation.

Thirty-minute formative journey targets, not measured promises: 0–3 sample; 3–8 select personal CSV/describe; 8–13 confirm columns/date/decimal/error rules; 13–18 key/dependency and outbound review; 18–25 staged result/checks; 25–30 save and run a second input. Fresh account setup can overrun: measure and disclose it, rather than excluding it from elapsed time. Before beta test five unfamiliar target users, including at least two non-developers: ≥4/5 complete the second-input run in thirty minutes with prerequisites available; all explain cloud/read/write scope and stop an active build; zero accidental grants/execution. Also test bad-column recovery and, before Show ships, denial of capture permissions followed by Tell. This is a formative gate, not a population success claim.

During later recording, show a floating timer, scope, Pause, Stop, and Discard. Explain that demonstrations use the real desktop and manual actions can have real effects; recommend disposable samples. Pause creates an explicit gap. At five minutes stop cleanly and offer a continuation draft, not unlimited background recording. Exclusions remain active during teaching. Instructions entered through Add note are user intent; visible third-party text remains untrusted evidence.

### 2.3 Menu-bar wireframe

This is the later expanded menu; private MVP shows Describe, saved CSV tools, active build status, Stop build/tools, Local-only, Privacy, Settings, and Quit. Hide Show, discovery, observation, and repair rows until shipped.

```text
+------------------------------------------------+
| Capture: Stopped          Tools: 0 running      |
| [Show a task] [Describe]    [Open Macroscope]    |
|------------------------------------------------|
| Pinned                                         |
| Clean import CSV                       [Run...] |
| Make branch name                       [Run...] |
|------------------------------------------------|
| Needs you: CSV repair ready            [Review] |
| Discoveries: 2 for weekly review        [Open]  |
|------------------------------------------------|
| [Pause observation...] [Stop build/tools]       |
| [What is captured?] [Privacy] [Settings] [Quit] |
+------------------------------------------------+
```

Use acknowledged states: Recording, Observing metadata, Paused by you, Excluded, Permission needed, Stopped. An overlay badge can indicate build attention without replacing the capture label. Closing a window hides that window; Quit stops capture, triggers, model hosts, PTYs, and runs. Pause observation stops observation only. Stop build/tools cancels queued/active builds and runtime process trees and disables triggers, using the model cancellation barrier in §2.7 for build hosts. Local-only stops every model job but permits deterministic tools. Emergency stop combines observation, model, and runtime barriers.

### 2.4 Discoveries and Opportunity detail wireframes

These wireframes are for the later discovery increment. Private-MVP drafts live in Tools; Discoveries is hidden. Later show at most three new cards/week and label metadata matches “Similar app sequence; teach it,” not a known transformation. Default system notification is off; offer one weekly review reminder. User-requested build approvals/failures are transactional, batched where possible. Avoid capture overlays narrating what the user is doing.

```text
+----------------+---------------------------+------------------------+
| Discoveries    | Review this week          | Selected opportunity   |
| Tools          | [All] [Taught] [Observed]  |                        |
| Builds       1 |                           | Similar export cleanup |
| Activity       | > Prepare import CSV      | on 3 covered workdays  |
| Privacy        |   4 examples; rule unclear|                        |
|                |   [Review evidence]       | [Inspect task]         |
| Pinned         |                           | [Show the real rule]   |
| Clean CSV      | Rename export batches     |                        |
|                | 5-item batch; native files|                        |
| Capture: Off   |                           | [Later] [Dismiss]      |
+----------------+---------------------------+------------------------+

+------------------------------------------------------------------+
| Prepare import CSV                          Source: Taught        |
| Goal [Map exported orders to the confirmed import format       ]  |
|                                                                  |
| Evidence: 4 local occurrences; 3 covered days; screenshots expired |
| Observed: permitted app sequence; export/edit/save unobserved      |
| Inferred: date rule ?         [Confirm YYYY-MM-DD]                 |
| You said: preserve every order; use decimal amounts               |
|                                                                  |
| INPUT  [Select representative CSV]     OUTPUT [Choose folder]      |
| RULES  columns [Order, Date, Amount]   bad dates [Stop and explain] |
| DONE   18 input rows -> 18 output rows; source unchanged           |
| ACCESS selected input; create one CSV in chosen folder            |
|                                                                  |
| Estimate: 4 x (3 min - 30 sec review) = about 10 min/week          |
| Basis: limited examples; confirm after real runs                   |
| [Edit steps] [Review outbound packet] [Build reviewed contract]   |
+------------------------------------------------------------------+
```

Do not show observed counts for a one-off description. Say “You described this task; recurrence unmeasured.” Candidate eligibility and build readiness differ: a candidate may warrant teaching but cannot build until inputs, rule, destination, and success checks are resolved. The accepted revision becomes immutable build input.

### 2.5 Builder with advanced transcript details

```text
+-----------------------------------------------------------------------+
| Clean import CSV   Writing -> Checking -> Compare       $0.48 / $2.00  |
| Task summary: map orders for import              [Stop build]          |
|-----------------------------------------------------------------------|
| RESULT   18 rows -> 18 rows                     [Compare result]      |
| RULES    dates MM/DD/YY -> YYYY-MM-DD; decimal USD; bad dates stop      |
| EXAMPLES CHECKED  normal rows, malformed date, missing amount          |
| NEEDS YOUR CHECK  are these the right columns and date meaning?        |
| ACCESS   read the selected CSV; create one CSV in Reports              |
|          source unchanged; existing destination refused               |
|                                                                       |
| [Try another sample] [Edit rules]                    [Save tool]       |
| [Advanced details: transcript / source / diffs / test cases / usage >]  |
+-----------------------------------------------------------------------+
```

Milestones come from host evidence. A completed Muse turn is “Candidate received,” not “Ready”: only independent validation produces Ready to test. Do not fabricate percentage completion. Render reasoning summaries only if supplied for display; never expose or promise private chain of thought.

A terminal is deferred. If later added, the key-free shell requires a workspace lease after stopping a build; edits invalidate test results. Disable terminal-controlled clipboard writes and automatic URL opening, isolate its renderer from product IPC, and cap scrollback. Muse TUI handoff additionally requires sandbox, key-isolation, and accounting tests. Release MSP ownership before `muse resume`, make structured UI read-only, and reconcile after exit. Never auto-type a secret into a terminal. All transcript/source/output surfaces obey §8.4.

### 2.6 Test, run, trust, and repair wireframes

```text
+------------------------------------------------------------------+
| Prove Clean import CSV                    candidate 7             |
| Sample: 18 rows -> 18 rows                 [Compare fields]        |
| PASS date/amount rules; original unchanged; existing target refused|
| Output remains staged. Live files have not been replaced.         |
| [Try another sample] [Edit rules]                    [Save tool]  |
+------------------------------------------------------------------+

+------------------------------------------------------------------+
| Run Clean import CSV                           version 7          |
| Read: [Choose orders.csv...]   Create: [Reports/orders_ready.csv]  |
| Plan: 18 rows; source unchanged; destination does not exist        |
| [Stage and compare]            [Cancel] [Create previewed CSV]    |
|                                                                  |
| Ask before each run (current)                                     |
| Every new input needs a fresh staged output and approval          |
| Folder auto-run: unavailable until separate trust review          |
+------------------------------------------------------------------+

+------------------------------------------------------------------+
| Clean import CSV stopped before writing                           |
| Expected column created_date; found created.                      |
| [Choose another input] [Correct mapping locally] [Prepare repair] |
| Repair packet: schema + error + synthetic row        [Review]     |
| Repair candidate 8: 7 checks pass; same permissions    [Compare]   |
| [Keep v7 paused] [Test v8] -> [Approve v8]      [Rollback]          |
+------------------------------------------------------------------+
```

Every MVP run provides a receipt with version, selected inputs, approved/committed byte digests, output link, and status. Commit the previewed staged bytes without rerunning generated code. General undo and the repair wireframe above are later work; initially offer choose another input, correct rules, or a new reviewed build. Three clean runs may later reduce navigation only for the same enforced scope; they neither prove safety on future data nor waive staged-byte approval. New/repaired code requires checks again. Authentication failures lead to key replacement, quota to waiting, and only code/data-shape failures to repair.

### 2.7 Privacy center wireframe

```text
+-----------------------------------------------------------------------+
| Privacy  Overview | Outbound | Retention | Exclusions | Grants | Delete |
| Capture: stopped [Pause / Resume]    Cloud analysis: off                |
| [Emergency stop capture, builds, and tools]                            |
|                                                                       |
| LOCAL DATA                 EXPIRES              SIZE                   |
| Sanitized frames           24 hours             28 MB                  |
| Detailed events/OCR        7 days                42 MB                  |
| Recurrence summaries       30 days               4 MB                   |
| Approved tools             until deleted        12 MB                  |
|                                                                       |
| OUTBOUND                                                              |
| 14:02 Direct Meta: contract analysis    text only    $0.02 [Payload]    |
| 14:04 Supplied to Muse: build packet    synthetic    $0.71 [Inputs]     |
| Muse makes additional requests from its workspace; wire bodies unknown.|
|                                                                       |
| [Delete last 15 min] [Delete app/site/range] [Delete captured history]   |
| [Export tools] [Revoke grants] [Replace / remove Meta key]              |
+-----------------------------------------------------------------------+
```

Local-only means no model egress: local mining and installed deterministic tools continue; generation and cloud explanations pause. It does not promise a local Meta model. The outbound viewer distinguishes exact direct-request payloads from app-supplied Muse inputs. Local deletion cannot recall provider-held data. Delete immediately without a misleading undo promise; preview the scope before a full-history wipe and offer a separate checkbox for tools.

Model egress has durable states `CloudAllowed(epoch)` → `Stopping(epoch+1)` → `LocalOnly`, plus `KeyUnavailable` and deletion tombstones. Every queued/admitted job and outbound packet carries the consent epoch and lineage IDs. On Local-only, close admission first, revoke the epoch, cancel queued jobs, terminate/reap all keyed Muse hosts and descendants (including reminder activity), cancel direct transports/retries, discard pending bodies, and reserve unknown charges for reconciliation. Show “Stopping cloud work…” until every worker acknowledges revocation and can no longer dispatch; then acknowledge Local-only. A failed barrier remains visibly stopping/blocked, never “local.” Already transmitted requests may complete and incur charges and cannot be recalled.

Deletion first tombstones the selected lineage, blocks derived jobs, revokes their epochs, and applies the same cancellation/reap barrier before destroying/rekeying local material. It covers pending packets, fixtures, outbound bodies, synthetic sessions, staging, undo/preimages, logs, and encryption keys, with restartable cleanup; intentional retained tools are listed separately and require an explicit retain choice after affected data is removed. User-owned exported outputs are listed as outside the app wipe unless separately selected. Crash recovery loads revocations before workers/jobs and never redispatches deleted or stale-epoch input. Test Local-only/Delete during analyzer dispatch, Muse tool/reminder work, transport retries, and crash recovery. Pause observation has no effect on admitted model work; Stop build/tools cancels those builds/runs; Local-only cancels all model work; Delete additionally removes selected local lineage. These distinctions must appear beside the controls.

The cloud review distinguishes synthetic/structural context from actual user data; schema, filenames, labels, and business rules can still be confidential. Show estimated/unknown cancellation charges and provider-retention limits beside consent. The privacy wireframe is the expanded product; in private MVP show fixtures, builds/tools, runs, outbound records, and key controls, without empty capture sections.

### 2.8 Complete interaction flow

```text
Describe (private MVP); bounded Show / local discovery later
  -> correct Task Contract + select local samples
  -> review analyzer packet + budget + cloud consent
  -> validate contract revision and capability ceiling
  -> build via selected Meta route with sanitized/synthetic project
  -> host tests + staged preview + compare
  -> approve immutable version + grant -> pin
  -> stage each selected input once -> approve exact bytes
  -> trusted broker commits staged bytes -> verify -> receipt
                          |
                       failure
                          v
  diagnose -> local mapping/auth fix OR reviewed repair packet
  -> new build workspace -> old + new tests -> permission diff
  -> explicit version approval -> supervised run / rollback
```

No arrow implicitly grants new access. Preserve drafts, edits, and staged source across auth, quota, crash, or budget failure unless deletion/revocation requires removing them. The repair branch and conflict-checked undo are later increments.

## 3. Architecture: processes, contracts, and persistence

### 3.1 Stack and process model

Use Tauri 2 with React/TypeScript/Vite, Rust/Tokio/Serde, bundled SQLCipher through a maintained Rust binding, and a Swift package for capture. Pin actual compatible versions at M0; do not copy unverified package versions or the unsupported CVE assertion from report 12 into dependency policy. Review primary advisories before choosing updater versions. Python can support local evaluation scripts but is not a shipped daemon.

The diagram is the target architecture. First product instantiates core/UI, vault/gateway, the selected bounded builder, validator, and staged-output broker/executor. Observer, mining, PTY, generalized effects/undo, and repair workers stay absent. A trusted handwritten converter may exercise the broker before generated execution is permitted; its success does not pass the generated-runtime gate.

Tauri uses system webviews; native framework integration and browser-engine differences remain test obligations. [VERIFIED-docs: Tauri process model, as cited in research 11.] Keep UI calls behind a generated typed Backend interface, so mocked browser tests exercise the same product state machine and a shell change would not rewrite domain code.

```text
                        TRUSTED PRODUCT CODE
+---------------------------------------------------------------------+
| Rust core: native tray, policy, one DB writer, key vault, job ledger   |
|     | Tauri IPC       | XPC            | private pipes                |
|     v                 v                v                             |
| React UI        Swift observer     analysis worker (on demand)        |
| no stored key   SCK/AX/Vision       sanitized packet -> Meta HTTPS    |
|                 raw in RAM only         |                            |
|     |                                  v                            |
|     |                         Muse supervisor -> muse serve           |
|     |                         restricted; shell off; API-key env      |
|     |                                                               |
|     +-> approved-version executor/broker (no Meta key)                |
|            | per-run private request channel                         |
+------------|--------------------------------------------------------+
             v                         UNTRUSTED GENERATED CODE
       Deno compute worker -> named SDK requests -> effect broker
       no user-data files, env, network, subprocess, capture authority
```

Core owns consent, grants, durable intents, and dispatch. Observer cannot call models or launch tools. Analysis workers get a job-specific sanitized packet, never archive-wide read access. Muse can edit only staging; it cannot publish or approve a tool. Executor checks immutable artifacts and grants; generated code cannot call Tauri product commands.

If MSP passes its gate, use one restricted Muse host while Builder is active, sharing sessions only sequentially and with a single workspace lease. Reap after ten idle minutes and immediately on lock, key removal/rejection, Local-only, applicable deletion, or Stop build. Direct transports obey the same egress epoch/barrier. No network-enabled build-tool posture ships in MVP. A second read-only host is added only when a real explanatory interaction requires it. Do not keep model processes resident during ordinary idle capture.

On macOS, core is the opt-in login item and owns observer lifetime. No independent KeepAlive recorder. Observer stops on control EOF or lease expiry; core crash must remove capture authority within one second. Close-window hides; explicit Quit stops the process tree and does not resurrect it. Windows uses a visible per-user startup task later, without a system service.

### 3.2 IPC and state reduction

Trusted UI uses narrowly scoped Tauri commands/channels, validated again in Rust. Native Mac observer uses authenticated XPC with signing requirements and a versioned message envelope; Windows observer uses an ACL-restricted named pipe with peer identity checks. On-demand analysis/executor children use inherited pipes. Own messages use bounded NDJSON JSON-RPC, one MiB per control message; images are separate bounded sanitized blobs by opaque ID. Muse uses its observed NDJSON protocol, never Content-Length framing.

All envelopes contain protocol/build version, request ID, sequence, policy epoch, job ID, and typed payload; model jobs additionally carry consent epoch and lineage IDs. Check peer/protocol before accepting content and recheck current epochs at transport dispatch and each broker effect. Cancellation and pause have priority over bulk updates; coalesce redundant frame events and report gaps. Unknown additive fields are tolerated; unknown required protocol versions stop that component. Errors contain stable codes and scrubbed summaries, not arbitrary exception dumps. Muse's internal requests cannot be individually epoch-gated, so revocation requires host termination/reaping, not just refusing another `turn/start`.

Persist externally consequential intent before invoking a broker effect. UI reconnect uses a snapshot and sequence cursor. Acknowledging a button click, admitting a job, executing an effect, and verifying output are distinct events. After crash, an unfinished mutation is outcomeUnknown until its journal/preimage/postimage is reconciled. Never automatically retry an uncertain write.

### 3.3 Storage and schema

One Rust storage actor owns metadata and, later, capture-shard writes. Worker results arrive as validated DTOs; no worker runs migrations. Use SQLCipher, memory-only SQL temporary storage, authenticated encrypted media, and random device-local IDs. No vector extension in MVP; later exact feature-vector scans suffice for a few thousand medoids and avoid extension packaging. Source-control only synthetic fixtures.

The following DDL is the minimum relational skeleton; production adds checks, indexes, and migrations. Detailed capture rows reside in daily encrypted shards with equivalent identifiers; long-lived metadata holds only sanitized aggregates and references.

This is a target skeleton, not an instruction to implement all tables in M1. Private MVP needs artifacts, tools, versions, grants, runs, jobs, usage, outbound, and a confirmed-task revision; capture events/shards, episodes/actions/clusters, and discovery feedback wait for capture admission. Persist consent epochs, lineage tombstones, grant revisions, and immutable staged-plan references alongside the relevant rows. Selective rekey/delete of shipped fixtures/builds/runs remains required; the capture-shard variant is deferred with capture storage.

```sql
CREATE TABLE artifacts (
 id TEXT PRIMARY KEY, kind TEXT NOT NULL, cipher_path TEXT NOT NULL,
 key_ref TEXT NOT NULL, policy_epoch INTEGER NOT NULL, expires_ms INTEGER);
CREATE TABLE events (
 id TEXT PRIMARY KEY, capture_run TEXT NOT NULL, producer TEXT NOT NULL,
 seq INTEGER NOT NULL, mono_ms INTEGER NOT NULL, wall_ms INTEGER NOT NULL,
 kind TEXT NOT NULL, app_token TEXT, payload_json TEXT NOT NULL,
 policy_epoch INTEGER NOT NULL, UNIQUE(capture_run, producer, seq));
CREATE TABLE episodes (
 id TEXT PRIMARY KEY, revision INTEGER NOT NULL,
 start_ms INTEGER NOT NULL, end_ms INTEGER,
 boundary_json TEXT NOT NULL, coverage_json TEXT NOT NULL);
CREATE TABLE actions (
 id TEXT PRIMARY KEY, episode_id TEXT NOT NULL, ordinal INTEGER NOT NULL,
 signature TEXT NOT NULL, detail_json TEXT NOT NULL);
CREATE TABLE evidence_links (
 owner_type TEXT, owner_id TEXT, ref_type TEXT, ref_id TEXT,
 PRIMARY KEY(owner_type, owner_id, ref_type, ref_id));
CREATE TABLE clusters (
 id TEXT PRIMARY KEY, normalizer_version TEXT NOT NULL,
 medoid_json TEXT NOT NULL, aggregates_json TEXT NOT NULL);
CREATE TABLE opportunities (
 id TEXT, revision INTEGER, state TEXT NOT NULL, contract_json TEXT NOT NULL,
 PRIMARY KEY(id, revision));
CREATE TABLE tools (
 id TEXT PRIMARY KEY, active_version TEXT, state TEXT NOT NULL);
CREATE TABLE versions (
 tool_id TEXT, hash TEXT, contract_hash TEXT NOT NULL, manifest_json TEXT NOT NULL,
 test_receipt TEXT NOT NULL, signature TEXT NOT NULL,
 PRIMARY KEY(tool_id, hash));
CREATE TABLE grants (
 id TEXT PRIMARY KEY, tool_id TEXT NOT NULL, version_hash TEXT NOT NULL,
 scope_json TEXT NOT NULL, revoked_ms INTEGER);
CREATE TABLE runs (
 id TEXT PRIMARY KEY, tool_id TEXT NOT NULL, version_hash TEXT NOT NULL,
 state TEXT NOT NULL, plan_hash TEXT, journal_ref TEXT, failure_class TEXT);
CREATE TABLE jobs (
 id TEXT PRIMARY KEY, kind TEXT NOT NULL, input_hash TEXT NOT NULL,
 state TEXT NOT NULL, cursor_json TEXT, reserved_usd REAL NOT NULL);
CREATE TABLE model_usage (
 leg_id TEXT PRIMARY KEY, parent_id TEXT, model TEXT, usage_json TEXT,
 price_version TEXT NOT NULL, estimated_usd REAL, reconciliation TEXT NOT NULL);
CREATE TABLE outbound (
 id TEXT PRIMARY KEY, purpose TEXT, payload_ref TEXT, scope TEXT NOT NULL,
 consent_revision TEXT NOT NULL, sent_ms INTEGER);
```

Foreign keys apply within each database; cross-shard evidence resolution goes through the storage actor and tolerates expired evidence. Add feedback and capability-registry tables when discovery ships. Separate schema versions from algorithm/prompt versions. Float USD is illustrative; use integer microdollars/decimal accounting in implementation.

Filesystem layout:

```text
Application Support/Macroscope/          Windows: %LOCALAPPDATA%/Macroscope/
  config/settings.json                  non-secret preferences only
  db/state.db                           encrypted metadata, jobs, grants, costs
  evidence/YYYY-MM-DD.db                 daily sanitized capture shard
  media/YYYY-MM-DD/<random>.enc          sanitized images, 24-hour expiry
  keys/wrapped/                         wrapped day/artifact keys, no plaintext key
  builds/<build-id>/                     synthetic-only git staging workspace
  muse/{config,data,state,home}/         isolated durable synthetic sessions
  tools/<id>/versions/<hash>/            immutable source, manifest, test receipt
  tool-data/<id>/                        mutable app data, independent of source
  runs/<id>/                            encrypted staging, journal, undo images
  tests/private/                        locked fixtures/oracles, outside agent repo
  cache/                                bounded disposable runtime/build cache
```

Daily keys limit retention and complete-day deletion. Selective deletion rewrites surviving shard/blob material under a fresh key before destroying the old key; SQL DELETE alone is insufficient. Use tombstones and a restartable GC journal so failed cleanup cannot reintroduce deleted evidence. Build source contains synthetic values; real fixtures, user paths, and preimages stay encrypted outside git. Never store a supposedly private fixture in git and merely add it to .gitignore afterward.

### 3.4 Repository and delivery tree

```text
apps/desktop/{src,src-tauri}/           UI, native tray, Tauri capability policy
crates/{domain,protocol,storage}/       types, IPC, encryption/deletion/migrations
crates/{coordinator,policy}/            process lifecycle and authority checks
crates/{patterns,model-gateway}/        local mining and direct Meta transport
crates/{muse-client,builder}/           MSP reducer, provider-neutral build API
crates/{executor,effect-broker}/        run lifecycle, validation, effects
crates/{observer-windows,pty}/          native Windows adapter, terminal support
native/macos-observer/                 Swift/XPC, SCK, AX, Vision, plists
packages/{sdk,ui,templates}/            effect SDK, shared components, starters
contracts/{observer,manifest,task,msp}/ pinned schemas and generated bindings
fixtures/{capture,msp,adversarial}/     synthetic replay and protocol transcripts
tests/{integration,desktop,evals}/     cross-process, packaged UI, quality suites
scripts/                              doctor, bindings, package, sign, verify
.github/workflows/                    PR checks, trusted platform runs, release
docs/adr/                             later implementation deviations from this plan
plan/                                 this plan and original evidence
```

Use Cargo/pnpm workspaces and Swift Package Manager. `doctor` checks SDKs, permissions, Muse version/fingerprint, vault availability, and credential presence without revealing content. Compile Windows pure-core/UI contracts from M0, but do not claim Windows capture passed when CI used a fake observer.

## 4. Meta models and Muse integration

### 4.1 Verified service contract and routing

The production provider is Meta. Primary docs confirm `https://api.meta.ai/v1` and Bearer API-key authentication; use a native Rust HTTP adapter, not a speculative Meta-specific npm SDK. [VERIFIED-docs: [Meta authentication](https://dev.meta.ai/docs/authentication).] Muse Wonder and Spark are documented API models; Wonder Full is subscription-only and is not a substitute for an API route. [VERIFIED-docs: [Meta models](https://dev.meta.ai/docs/models).]

Do not build around maximum context capacity. Bound job packets and keep the source evidence local. Glimmer, SAM, image generation, voice transcription, and consumer products have no v1 role. Self-hosted Meta inference is excluded by the API-billed-model requirement even if it becomes technically convenient. OCR and deterministic local algorithms are permitted preprocessing, not an alternate foundation-model provider.

| Job | Route/model | Bound and input | Failure behavior |
|---|---|---|---|
| Capture, masking, segmentation, recurrence | No model | Local structural signals | Partial evidence/gap; no cloud fallback |
| Task Contract interpretation | Direct Responses, `muse-spark-1.3`, low effort | Sanitized actions and user-approved rule; 8k input/2k output target | One schema correction; then local editable draft |
| Explain discovery shortlist | Same Spark route | At most five changed candidates/day, 4k input/1k output each; images off | Deterministic title and evidence remain available |
| Build SDK workflow | Muse MSP, `muse-wonder-1.0`, bounded medium effort | Approved contract, SDK documentation, synthetic samples, scrubbed test feedback | Stop on budget; explicit Spark alternative only after its eval passes |
| Build fallback when MSP gate fails | Direct Responses, `muse-wonder-1.0`, zero model tools | Same approved contract/SDK/synthetic packet; strict source/manifest response | Same independent validation; no host coding loop or shell escalation |
| Repair SDK workflow | Same Wonder host and policy | Source + sanitized failure schema + synthetic regression | Two candidate attempts max; preserve old version |
| Key test | Catalog check, then one tiny Spark turn | Fixed synthetic prompt, separate explicit paid test | Typed auth/billing/network state |
| Runtime deterministic tool | No model | User-selected local data | Fail closed on malformed input |
| Runtime LLM node | Disabled | No request admitted in v1 | Requires later capability/egress design |

A route change is visible and eval-gated. Never silently switch providers, Contributor variants, or subscription-backed models. If Wonder is unavailable, offer a reviewed Spark build or defer; do not keep retrying model IDs. Discovery never needs a weekly Wonder pass.

For direct `POST /v1/responses` requests use `store:false`, an explicit Standard model/output ceiling, and structured output configured through **`text.format`**. Top-level `response_format` may be compatibility-accepted without enforcing structured output; do not copy Chat Completions nesting. [VERIFIED-docs: [Responses compatibility notes](https://dev.meta.ai/docs/protocols/responses).] M0 must freeze a successful synthetic request/response fixture, strict schema/dialect, usage, output truncation/refusal, auth/quota/malformed-output handling, and independent host validation; live compatibility and image metering remain UNVERIFIED. A 413 shrinks the packet; do not introduce persistent uploads to bypass limits. Start with text below 512 KiB; reviewed image crops (at most two, one MiB combined) belong to the later capture track.

### 4.2 Provider abstraction

Keep single-shot model requests separate from a coding session:

```ts
interface ModelGateway {
  generate<I, O>(task: TaskId, packet: ClearedPacket<I>,
    consent: ConsentRef, budget: BudgetReservation): Promise<Validated<O>>;
}
interface CodingAgent {
  start(req: BuildRequest): AsyncIterable<BuildEvent>;
  resume(ref: BuildRef): AsyncIterable<BuildEvent>;
  answer(ref: BuildRef, decision: ScopedDecision): Promise<void>;
  cancel(ref: BuildRef): Promise<void>;
}
// BuildRequest: contractHash, syntheticWorkspaceHandle, permissionCeiling,
// modelRoute, credentialRef, cost/time/attempt limits.
// BuildEvent: phase, message, artifactChanged, approval, inputRequest,
// usage, interrupted, candidateCompleted, failed.
```

The gateway validates provenance, consent revision, destination, model policy, and budget before transport. Model output cannot modify routes or create capabilities. Add adapters only when required: MetaResponses and MuseMsp initially, MuseExec as a bounded diagnostic/recovery lane. A direct Meta artifact generator is the contingency if Muse containment or deployment is incompatible; it returns source/manifest data and gives the model zero host tools. It does not recreate a general coding agent.

At M0 compare the documented TypeScript Muse SDK (`@muse-code/sdk`) with a minimal Rust MSP adapter for deployment burden, API-key routing, profile/tool control, cancellation, usage, and schema maintenance. [VERIFIED-docs: [Muse programmatic integration](https://dev.meta.ai/docs/muse-code).] Compatibility with this binary/schema and the required boundaries is UNVERIFIED; record a decision artifact and prefer the SDK only if it removes work without losing enforcement. Do not implement the full exported protocol just because it exists: the selected route needs only its tested product subset.

### 4.3 `muse serve` hosting and protocol

Spawn the tested native Muse binary directly, bypassing its updating launcher. Do not bundle it. Current terms distinguish using outputs from incorporating downloadable materials, so redistribution needs explicit clearance rather than an optimistic assumption. [VERIFIED-docs: [Meta terms, section 1](https://dev.meta.ai/legal/terms-of-service).] Check installed path, signature, version, MSP version, and fingerprint; re-resolve only between jobs because the external launcher can remove old versions.

Default host invocation is an argument array equivalent to:

```text
<resolved-muse-binary> serve
  --provider meta
  --model muse-wonder-1.0
  --contributor-mode off
  --sandbox-network restricted
  --disable-shell
```

Use durable sessions for an admitted product integration, omitting `--no-session-log`; logs contain synthetic context only and follow deletion rules. The invocation is a candidate, not a working approved posture. Before a real-key spawn, reproduce/classify probe 21's `permission_profile_reviewer_unavailable` with synthetic/bogus-key tests. Select an available permission profile through a surface demonstrated on the pinned binary, record its effective policy, and explicitly test `approvalMode:"promptUnmatched"` rather than assuming it fixes profile admission. The schema distinguishes `approvalMode` from `permissionProfile`; the latter's comment mentions `permissionProfile/list`, but its typed method surface is not established by this export. Ephemeral profiles also have different admission rules. [VERIFIED-local: [probe 21](probes/transcripts/21-goodkey.jsonl), [SessionStartParams](reference/msp/msp.d.ts).] Do not invent config keys, treat an approval mode as a profile, or change the owner's global configuration. The app denies requests outside its ceiling. Disabling the automatic reviewer is UNVERIFIED for serve; if it cannot be disabled its authority, egress, and usage must be included in containment/accounting evidence.

The supplied serve help does **not** list `--disable-web-tools`, `--no-foreign-personal-context`, `--disable-reminders`, `--approval-judge`, or `--base-url`. Never copy those exec flags into serve. Unknown flags/config keys prevent startup. Restricted network was observed blocking shell curl; this is not proof that every built-in web tool or model request is intercepted. M0 must enumerate effective tools and demonstrate the required boundary.

Set XDG config/data/state and a private HOME to app-owned Muse directories; provide only the minimal app skill pack. Private HOME intentionally sacrifices personal git/toolchain conveniences because builds do not need them. XDG alone was observed leaking foreign personal skills. Set child-only `MUSE_LOGIN=0` and `MUSE_NO_AUTO_UPDATE=1` as additional defenses if launcher code is ever reached; invoking the pinned binary directly remains the default. Never copy the whole parent environment. Do not load workspace rules: omit `--trust-workspace`, reject generated rule/hook/plugin files, and keep app instructions outside agent-editable source.

**Containment gate:** with synthetic fixtures and a canary credential, demonstrate that shell is unavailable; file tools cannot read real home, capture history, app key files, or locked tests; tools cannot access undeclared destinations; and plugins/subagents cannot reopen these routes. The exact outer OS sandbox integration is UNVERIFIED. If native file tools are insufficiently scoped, wrap the host with a tested read-confinement profile. If this still fails, use the zero-tool direct artifact path for shipping and keep interactive Muse integration experimental. Do not solve failure by asking users to approve broader desktop access.

The serve help labels filesystem/network sandboxing as **shell** confinement. Test native reads/writes, built-in web tools, plugin/hook loading, child/reminder/reviewer routes, env exposure, and egress separately under the exact private HOME/profile/launch. `--disable-shell` and restricted shell network alone do not pass. Use canary credentials until denial is demonstrated; real paid smoke calls are fixed synthetic, zero-effect turns only after that gate. Inaccessible profile or failed confinement selects direct Meta zero-tool generation immediately; dependent MSP UI work waits.

VERIFIED-local protocol behavior from report 04: newline-delimited JSON-RPC; `initialize` followed by `initialized`; start/turn acknowledgment is not completion; final events must match the authoritative acknowledged turn ID. Match item identity/revision, append deltas, replace with the newer full completed item, and retain opaque view cursors. Unknown item kinds render generic fallback text. Unknown methods/fields must not crash the reducer.

Pin schema v1 and the full fingerprint from `reference/msp/manifest.json`. A changed fingerprint triggers recorded/echo conformance checks before permitting builds; a major schema mismatch blocks. Echo validates transport and reduction, not live approvals or model-tool safety. Crash recovery resumes the session and reconciles the orphaned terminal before admitting another turn. Never interpret a stale terminal as the current turn's result.

### 4.4 MSP-to-UI mapping

| MSP surface | Product behavior | Evidence and implementation rule |
|---|---|---|
| `initialize`, `model/list` | Builder health/model availability | Observed; check sandbox and catalog source, not only version string |
| `session/start`, `session/resume` | New build or recovery | Observed; persist backend session and workspace mapping |
| `turn/start`, acknowledgment | Composer submit / queued work | Observed; retain returned turn ID and command ID |
| `item/started`, `item/delta`, `item/completed` | Streaming Muse transcript and tool rows | Observed; revisions and generic unknown-item handling |
| `toolCall`, patch references | Files changed and expandable diff | Schema; host filesystem diff remains authoritative |
| `turn/foregroundCompleted` | Main response complete; checking continues | Observed reminders can follow; not Ready |
| `turn/completed` | Candidate complete, auth error, cancelled, failure | Observed auth/error path; live successful usage unverified |
| `approval/request` | Pinned review card | Schema-only live flow; receipt is immediate, decision separate |
| `approval/decide`, `approval/updated/resolved` | Scoped user decision and race reconciliation | Include current requirement ID; stale decision refreshes card |
| `userInput/request` | Inline question | Schema; cannot expand runtime grants |
| `session/tokenUsage`, context usage | Estimated cost/context footer | Echo/catalog observed; paid totals and child coverage unverified |
| `turn/interrupt`, `turn/steer` | Stop / add note | Schema; cancel admission as well as current work |
| `view/gap`, `view/page`, session snapshot | Repair missing transcript after reconnect | Schema; IDs/cursors persisted opaquely |
| `goal/*`, `subagent/*`, tasks/plugins | Disabled product features initially | Autonomous turns and child billing complicate the budget/security boundary |
| `session/userShell` | No MVP agent shell | Observed to inherit key; any later key-free product terminal is separate |
| `session/fork` | Optional conversation branch | Observed full fork shares workspace; never equate it with source rollback |

For approvals, acknowledge presentation immediately, then apply product policy and display allowed narrow choices. Never expose persistent approval rules in MVP. Block requests above the policy ceiling even if Muse offers Allow. An unknown subject receives denial/explanation. A notification opens review; it does not authorize the action.

Source rollback uses immutable published versions. “Try another approach” creates a new workspace and session. Do not implement report 04's hard-reset-plus-approximate-conversation rewind as the primary UX: it risks destroying developer edits and presenting misleading context.

### 4.5 API-key authentication: verified versus unverified

**This subsection takes precedence over older assertions in reports 03, 05–15 that serve environment inheritance remains untested.** The governing evidence is [04-muse-msp.md, section 0](research/04-muse-msp.md#0-api-key-auth-verified-behaviour) and the indexed [probe transcripts](probes/README.md).

| Finding | Status | Consequence |
|---|---|---|
| Bogus `META_API_KEY` reaches Meta from `muse serve` | VERIFIED-local, probes A/B | Serve reads the child environment; that brief-level uncertainty is closed |
| Warm real-home failure arrives at `turn/completed.error.kind=authRequired` | VERIFIED-local | Observe turn terminal, not just RPC result or stderr |
| Fresh isolated home can return empty bundled catalog and misleading “not logged in” | VERIFIED-local | Do not tell users to consumer-login; classify initialization diagnostics separately |
| `exec --api-key-stdin` is the same key lane | VERIFIED-local, D–G | Use stdin for exec, reserving serve stdin for MSP |
| Fresh-home exec may exit before any JSONL | VERIFIED-local | Capture exit and scrubbed stderr as well as stdout |
| Fake exec endpoint sees Bearer credential on catalog GET | VERIFIED-local, G | Stdin avoids argv exposure but does not change provider authentication |
| Shared account mutation clobbered personal credential | VERIFIED-local, section 0.3 | Never call `account/*`, login/logout, or auth-set from Macroscope |
| `session/userShell env` contains the key | VERIFIED-local, section 0.4 | No key-bearing shell in MVP; stdin alone is not secret isolation |
| Serve honors `--base-url` | Refuted locally | Do not propose an implemented key-injecting proxy for serve |
| `21-goodkey.jsonl`: provider catalog, then unavailable `:auto-review` profile and rejected `session/start` | VERIFIED-local; absent from the older README index | No turn ran; filename/catalog do not prove inference or API-key billing |
| Probe 21 supplied the app's environment key | Not established: `env_overrides:{}`; harness removes inherited `META_API_KEY` | Cannot use this artifact to prove app-key auth or no account fallback |
| Successful real-key turn, invoice attribution, usage totals, reminder costs | UNVERIFIED | Must pass M0/M1 before paid generation is offered |
| Live approvals, quota behavior, Windows credential/sandbox path | Schema/docs only, UNVERIFIED-runtime | Exercise explicitly before platform release |

The published auth precedence is environment key, stored key, then browser session. [VERIFIED-docs: [Muse authentication](https://dev.meta.ai/docs/muse-code/auth).] The app nevertheless blocks model admission when its own vault reference is missing, rather than relying on that precedence to stop fallback.

```text
Native secure key entry
        |
        v
Macroscope-owned Keychain / Credential Manager item
        | credentialRef only -> UI/database
        |
        +-> native ModelGateway -> HTTPS Authorization: Bearer key -> Meta API
        |
        +-> MuseSupervisor -> child-only META_API_KEY -> muse serve -> Meta API
        |                    stdin/stdout remain MSP
        |
        +-> bounded exec -> private --api-key-stdin pipe -> Meta API

Never -> observer, generated code, runtime env, argv, prompt, fixture, PTY, logs
```

Store a dedicated application key under an app-owned service/account, device-only/non-synchronizing on Mac. The native entry sheet avoids persisting the key in web state; clear transient fields immediately. The core retrieves it only for a request or spawn, uses an allowlisted child environment, and removes unrelated credentials. App processes never mutate the owner's Muse auth store. A dedicated API key limits revocation blast radius; per-key spend/model scopes remain UNVERIFIED, so do not advertise them.

Catalog success establishes catalog availability, not proven paid inference or billing identity. Empty catalog alone can also mean transport/catalog trouble; show Key rejected only with typed auth or corroborating rejected-credential diagnostics. A real-key smoke test requires a fresh private HOME/XDG tree, app vault key, explicit Standard Spark, successful exact-session/turn completion, usage, and owner API-dashboard attribution. Repeat with an existing personal session and a rejected app key to prove no successful fallback. Never invoke account/auth mutation to create that control.

**Dedicated smoke harness prerequisite:** do not run research 04 §0.2's `--env META_API_KEY=$KEY` command. Shell expansion places the key in Python argv; transcript metadata redaction does not remove that exposure. The historical harness copies ambient env, defaults to approval, records raw frames/stderr, and accepts any terminal. [VERIFIED-local: [probe client](probes/msp_probe.py), [research 04](research/04-muse-msp.md#02-successful-call-with-a-real-key-not-run).] Build a separate harness with native secure input or an inherited private credential pipe, no secret argv/history, an allowlisted environment, private bounded storage, scrub-before-write stdout/stderr, explicit model/posture, deadlines/kill/reap, and deny-by-default approvals (no first-choice fallback). Match RPC IDs and acknowledged session/turn IDs; unrelated/stale terminals cannot count as success. Seed bogus-key canaries to verify argv/env/log redaction and denied tools before supplying a real key. Record commands with placeholders, binary/schema/profile fingerprints, and passed/failed/unknown evidence. This revision does not rerun or repair the historical probes.

Rotation stops admission, applies the cancellation/reap barrier, respawns with the replacement, then offers reviewed session recovery. Removal/auth rejection uses `KeyUnavailable`, cancels active/queued paid work, and never retries automatically. Local-only/deletion take precedence over recovery and quota retry; every dispatch rechecks epochs. Unknown usage keeps its budget reservation until reconciled. The key may be present inside the closed Muse host: shell-off and containment reduce exposure but do not establish perfect key isolation. General keyed shell/TUI stays blocked until technically resolved.

The historical probe changed the owner's login at research time. Current personal-login health is UNVERIFIED; this plan does not alter it. If the overnight keeper uses that credential, the owner should check it independently of Macroscope.

## 5. Capture: scoped observation with explicit gaps

This entire observer track is deferred from the first Tell product. M0 may run a separate signed synthetic capture experiment; passing it does not authorize capture storage or discovery in MVP. The requirements below apply before those features are exposed.

### 5.1 Shared observer contract

The observer produces sanitized facts, not execution instructions. A selected window is not permission to read all of its application's documents. Never infer “clicked Export” from a screenshot containing an Export button. Track source, confidence, and observation interval so downstream code distinguishes actions, states, and missing context.

```ts
interface Observer {
  capabilities(): CapabilitySnapshot;
  permissions(): PermissionSnapshot;
  start(scope: SelectedScope, policy: CapturePolicy): Promise<SessionRef>;
  setPolicy(policy: CapturePolicy): Promise<Barrier>;
  pause(reason: PauseReason): Promise<Barrier>;
  snapshot(scope: SelectedScope): Promise<ClearedObservation | Gap>;
  stop(): Promise<Barrier>;
}
interface ObservationEnvelope {
  version: 1; captureRun: string; producer: string; seq: number;
  monotonicMs: number; utcAnchorMs: number; policyEpoch: number;
  contextGeneration: number; source: "ax"|"uia"|"ocr"|"window"|"browser";
  coverage: "complete"|"partial"|"unavailable";
  event: Foreground | SemanticState | ClearedFrame | Gap;
}
// ClearedFrame includes geometry/transform, mask reasons, detector version,
// scope identity and privacy receipt; never the original masked values.
```

Only the privacy gate can construct cleared types; receipts are implementation evidence, not a proof that no sensitive content remains. Native handles stay native. Window identity includes process instance/generation to handle identifier reuse. Signed physical-pixel geometry supports negative coordinates, mixed DPI, and per-frame transforms. Drop associations when state/tree intervals are too far apart; start with a 250 ms correlation target.

Pause acknowledgment is a barrier: invalidate epoch, stop admission, revoke queued buffers, and ensure consumers discard earlier results. Raw candidates remain in memory only, with two latest-frame slots and a one-second quarantine for asynchronous policy rechecks. All queues are bounded. No recording code path starts merely because the app launches.

### 5.2 macOS implementation

Implement ScreenCaptureKit window streams, NSWorkspace focus/session signals, bounded AX reads, and Vision OCR in Swift. These are documented native surfaces, as detailed with primary links in [research 06](research/06-capture-macos.md). The integrated behavior remains UNVERIFIED until tested in the signed bundle. Support Apple Silicon on macOS 15 and 26 explicitly; do not make incidental API availability a macOS 14 support promise.

Use the system picker for initial selected-window capture. If a task requires crossing native windows, the user explicitly adds allowed windows or records a second segment; automatic focus-following is later. On focus loss, ambiguous AX-to-SCWindow mapping, minimized scope, stale geometry, secure input, lock, or observer error, close the content gate and emit a gap. Never silently capture a whole display to recover a missing panel.

Record separate signed-bundle experiments for picker-only selection versus broader Screen Recording preflight/request on each claimed macOS version, including helper attribution, relaunch, revocation, and N-1 update. The picker selects content for that session; do not treat it as a permanent grant to future windows. Exact permission behavior remains UNVERIFIED. [VERIFIED-docs: [Apple content sharing picker](https://developer.apple.com/documentation/screencapturekit/sccontentsharingpicker); local experiment still required.]

Start with SDR BGRA, hidden cursor, audio disabled, queue depth three, at most two candidate frames/second, and 1600-pixel maximum long edge locally. Normalize, mask editable/protected controls, compare sanitized thumbnails/tiles, debounce around 500 ms, and retain at most one routine frame per two seconds. Target six retained frames/minute during teaching, not a constant recording rate. Animations and scrolling coalesce. OCR runs off the stream callback, one worker, with explicit work tokens.

Read focused-window structure, roles, safe labels, bounds, and at most 150 nodes/depth eight per snapshot. Use bounded AX messaging timeouts and a total traversal deadline. Do not read editable AX values, selected text, document bodies, or addresses while being typed. If safe field geometry is missing, strict image mode drops the frame; no AX does not mean no secrets.

Browsers, terminals, messaging/call apps, authentication/finance/health apps, and remote-desktop clients are excluded by default. The initial browser exclusion is deliberately broad: private-mode title heuristics are not reliable enough. Later Chromium MV3 integration uses explicit optional origin grants, private mode disabled, trusted foreground-tab metadata, sanitized DOM labels, no values/cookies/storage bodies, and a signed native messaging bridge. Browser pixels remain off even after structured events work. Safari comes after Chromium; arbitrary AppleScript JavaScript injection is not a shortcut.

No global keyboard tap, ordinary keycodes, typed content, clipboard polling, or input-monitoring permission. If safe observation cannot distinguish typing or copying, omit those fields rather than infer them. Explicit clipboard input to a user-run tool is a separate broker capability, shown in its preview.

### 5.3 Permission and lifecycle sequence

| Step | Request/check | On denial or revocation |
|---|---|---|
| App-level capture scope | User chooses Show/allowed window | Remain Tell/sample only |
| Accessibility | User action invokes trust request; bounded real read verifies health | No automatic native frames requiring geometry; Tell still works |
| Screen access | Picker plus documented preflight/request path as needed | Structured-only recording when meaningful; otherwise explicit gap |
| Relaunch | Save draft and explain OS requirement if observed | Return to same step and recheck actual access |
| Future browser origin | Separate extension/host permission | No page content or screenshot fallback |
| Future Automation recipe | Per-target Apple Events explanation/consent | Only that recipe unavailable |
| Startup | Explicit login-item toggle after useful trial | User disable respected; no hidden re-registration |

The observer's signed bundle identity and main-app attribution must be demonstrated, including N-1 update and fresh-account grant/revoke. Notarization grants no TCC authority. macOS recurring capture confirmations are documented historically, but their exact cadence and wording on current systems are UNVERIFIED; test a long dogfood run and explain the actual behavior. Never suppress OS indicators or edit TCC databases.

### 5.4 Windows design and verification

Target serviced Windows 11 x64, signed MSIX, normal user. Use WGC through an explicitly selected window and keep its normal border. Use WinEvent for focus, a dedicated restartable UIA helper for potentially hung COM providers, and Windows.Media.Ocr under package identity. No global keyboard hook, administrator observer, UIAccess, secure-desktop access, or DRM bypass. Job Objects supply process/resource control, not file isolation.

A Windows permission snapshot includes NotRequiredByOs separately from app consent. Preserve all content-free gaps on lock, UAC/desktop switch, unknown elevation, missing language, device loss, or provider timeout. Start UIA at 500 nodes/depth twelve with a 500 ms watchdog; its larger initial cap is platform-specific, while exported common DTOs remain bounded. Failed OCR required for clearing causes a dropped frame.

Official Muse docs now publish Windows installation and platform support, resolving the absence-of-installer assumption in reports 04/12. [VERIFIED-docs: [Muse Code platform support](https://dev.meta.ai/docs/muse-code).] This does not establish successful Windows MSP, sandbox, XDG isolation, ConPTY, or package integration. Do not ship a “lite” homegrown general agent merely because those tests are pending.

MSIX is a custom delivery pipeline here: Tauri documents WiX/MSI and NSIS installers, not an MSIX bundle switch. Package identity/full-trust desktop activation does not confine Deno in AppContainer. [VERIFIED-docs: [Tauri Windows installers](https://v2.tauri.app/distribute/windows-installer/), [AppContainer launch](https://learn.microsoft.com/en-us/windows/win32/secauthz/implementing-an-appcontainer); see also [research 07](research/07-capture-windows.md).] Before scheduling Windows delivery, prove Tauri executable → signed MSIX identity/activation → OCR synthetic image → selected-window WGC → bounded UIA → external Muse private-home start/API auth (or selected direct fallback) → separately confined Deno → broker pipe → signed update. Record package manifest, activation/trust levels, AppContainer SID/token/capability/ACL setup, pipe ACL, exact launch, and denied-effect artifacts. Observer stays medium-integrity; worker confinement is separate. This chain is UNVERIFIED and needs funded interactive x64 infrastructure plus a physical tester.

Muse docs allow a possible first-time sandbox UAC setup prompt while normal use stays unelevated. Distinguish that documented setup from running an elevated observer: no hidden elevation or elevation to bypass a denial. Record the setup behavior/decision during the Windows spike; an unacceptable setup or failed boundary selects zero-tool generation, not an elevated application. [VERIFIED-docs: [Muse Windows platform differences](https://dev.meta.ai/docs/muse-code).]

Use hosted CI for compile/replay/schema/package checks; an agent-operated interactive Windows environment for picker, UIA, OCR, consent, update and cancellation; and an independent physical x64 tester for GPU, border, DPI, sleep/wake, Defender, and SmartScreen. The owner can inspect recorded synthetic test artifacts from Mac. If no tester is funded, Windows remains an explicitly incomplete preview. ARM64 is a later separately tested target.

### 5.5 Capture budgets

Initial Mac acceptance targets: active observer/OCR CPU under 3% of one logical core averaged over representative work; quiescent under 0.5%; observer footprint under 200 MiB; selected sanitized observation normally under two seconds; pause barrier p95 under 250 ms and stale buffers gone within one second. Measure induced WindowServer/OCR service cost, not just helper CPU.

Cap the evidence store at one GiB, images first to evict, detailed event ingest at 50 MiB/day, and image ingest at 100 MiB/day. Hitting a quota reduces images then stops collection with a visible reason. Teach sessions are capped at five minutes and public-v1 metadata capture has no continuous image store. These bounds replace incompatible 3–10 GB default archives in the research.

## 6. Pattern engine: local evidence before explanation

### 6.1 Canonical data and segmentation

Teach sessions create an explicit episode with user-provided intent. Ambient events begin without that label. Emit only signals in the provenance matrix below; every record carries source, tested adapter version, coverage, ambiguity, and state-versus-action kind. No integrated apps have been tested in these supplied artifacts. Before mining admission, fill actual app/OS/version results; unknown support stays absent.

| Emitted fact/verb | Exact proposed source | Permission / apps tested | Coverage and ambiguity |
|---|---|---|---|
| `app_focus`, `window_state` | macOS `NSWorkspace.didActivateApplicationNotification` + bounded focused-window AX query; Windows foreground WinEvent + bounded UIA | AX where needed / none integrated | State/transition only; not proof of a click, save, or export |
| `control_state` | Explicitly registered AX notifications plus bounded role/state read; Windows UIA property/event subscription | AX/UIA / none integrated | Only supported notifications, missing events are gaps; AX press action is not a user click log |
| `frame_changed`, `ocr_label` | Selected-window SCK + sanitized Vision; WGC + Windows OCR | Screen selection + redaction prerequisites / none integrated | Observed state, never inferred activation or field contents |
| `scope_gap`, `pause`, `session_inactive` | Observer barrier; NSWorkspace session resignation/screen/system sleep; Windows session/desktop notifications | App consent / none integrated | Content-free boundary, never evidence of a business step |
| `lock` | Mac public lock-state signal not yet selected/validated; Windows session-lock notification adapter | Capture blocked until validated / none integrated | NSWorkspace session inactivity alone is not proved lock detection; uncertain state stays paused |
| `step_confirmed` | Deliberate Add note / task-summary correction | User gesture / UI not built | User instruction; separate from passive observations |
| `file_selected`, `output_created` | Product native picker and trusted broker receipt | Explicit file grant / UI not built | Only actions inside Macroscope; cannot label another app's export |
| `browser_route`, `table_selected` | Later MV3 companion/native messaging with explicit origin grant | Extension origin grant / none integrated | No browser events until adapter passes; no private-mode/content fallback |
| `export_completed`, `download_ready`, `column_mapping`, `control_activated` in third-party apps | No selected adapter | Unsupported | Omit; no watcher, click tap, or dataflow source is in the MVP |

Research 06 proposes CGEventTap, but this plan does not currently adopt a mouse or keyboard tap. AX focus/notifications cannot replace that source with an invented activation log. Any future mouse-only adapter requires a deliberate scope decision, permission/secure-context tests, and its own provenance rows; do not expand collection to hit a mining metric. [VERIFIED-local: [macOS research](research/06-capture-macos.md), [pattern research](research/09-pattern-engine.md); proposed adapter coverage remains UNVERIFIED.]

Deduplicate `(captureRun, producer, seq)`, batch writes every 250 ms/100 events, and merge streams using a 500 ms reorder window. Delayed sanitized OCR may revise an episode but cannot bridge a protected gap. Keep monotonic duration and UTC/timezone anchors distinct; sleep, clock jumps, and policy changes are boundaries.

Hard-close episodes at pause, lock, permission loss, or two minutes idle; use a thirty-minute cap. Treat an app switch as a provisional segment: join only with supported document/task evidence or an explicit teacher correction, otherwise close after 45 seconds of unrelated activity. No clipboard HMAC bridge in v1. Preserve uncertainty instead of inventing a cross-app thread.

Estimate active duration by summing inter-action gaps capped at ten seconds; keep waiting and elapsed duration separate. This underestimates reading-heavy work, so expose a range and allow a user baseline. Do not interpret undo as emotion or cognitive stress. Friction is a task-structure ranking aid, never a personal evaluation.

### 6.2 Mining and ranking algorithm

Before implementing mining, manually perform two target tasks and matched negative controls using only the allowed sources, inspect the actual emitted sequences, and record whether they distinguish tasks. If only similar foreground-app sequences survive, show at most “Similar app sequence; teach it” in a research UI; do not claim action/dataflow understanding. If no useful distinction survives, keep Tell/Teach and postpone discovery without broadening collection. Current observation is not assumed to pass this gate.

The following algorithm is conditional on proven semantic signals: mine normalized n-grams of 3–12 supported actions, then extend overlapping seeds into closed candidates up to sixty actions. Require three non-overlapping occurrences across two episodes and either two dates or a five-item batch. Suppress navigation-only patterns. Distinguish item repeats inside a batch from independent task executions. Metadata-only prompts have a separate evaluation; do not score them as buildable transformation discoveries.

Compare variants only within compatible app/target/dataflow buckets, initially weighted edit distance at most 0.20. A changed write destination or reversed transfer vetoes merging; do not let transitive similarity join unrelated tasks. Feature-hash vectors can propose neighbors; exact sequence/target checks establish support. Add PrefixSpan or a learned embedding provider only if a held-out recall failure justifies the work.

Maintain a capability registry with operation, OS/app version, test receipt, expiry, and essential-step coverage. Supported file/table transforms receive the strongest initial reliability prior; browser/native control not shipped is zero. A model cannot declare an unsupported operation feasible.

```text
weekly_rate = executions / covered_workdays * 5  # only after >=5 covered days
base_effort = weekly_rate * median_active_minutes
friction = base_effort * capped_retry_and_switch_factor
A = essential_step_coverage * weakest_verified_step_reliability
Q = min(capture_coverage, alignment_consistency, identity_quality, support)
rank = friction * A * Q * bounded_user_preference
net_savings = weekly_rate * max(manual_minutes - review_and_run_minutes, 0)
```

Build readiness requires explicit input/rule/output confirmation and complete essential-step coverage; metadata similarity alone never meets it. For later buildable ambient cards, require A at least 0.60, Q at least 0.65, and projected saving of ten minutes/week or five minutes/batch. These tunable thresholds cannot create missing signals. Until five covered days, show batch totals rather than weekly extrapolation. Weekly scheduling claims require four observed weeks; a seven-day event store supports this only through retained coarse counts, never fabricated surviving screenshots.

### 6.3 Opportunity and builder handoff schema

```json
{
  "schema": "macroscope.opportunity/v1",
  "id": "opp_7",
  "revision": 3,
  "source": "taught",
  "state": "needs_confirmation",
  "goal": {"text": "Prepare an import CSV", "origin": "user"},
  "steps": [
    {"id": "s1", "verb": "map_columns", "origin": "confirmed",
     "evidenceIds": ["action_12"], "capabilityId": "table.map.v1"}
  ],
  "recurrence": {"executions": null, "coveredDays": 0},
  "samples": {"localFixtureRefs": ["fx_1"], "cloudCopies": "synthetic_only"},
  "unknowns": ["date interpretation when day and month are ambiguous"],
  "acceptance": ["source unchanged", "row count preserved"],
  "permissionCeiling": {"read": ["selected_input"], "create": ["selected_output"]},
  "evidenceExpiry": "2026-10-24T00:00:00Z",
  "provenance": {"normalizer": "v1", "policy": 4, "model": null}
}
```

Task Contract is the confirmed revision derived from this object: exact input/output schemas, error policy, mapping, acceptance checks, and user-selected capability bindings. Ambient counts are nullable for descriptions. Expired evidence remains labeled expired; it cannot be reconstructed from retained model prose.

Call Spark only for a new/materially changed shortlist or explicit task interpretation. It may propose a name, describe observed steps, and ask rule questions. Reject invented evidence IDs/capabilities and any computed metric changes. Cache by packet hash, prompt, model, and policy version. A schema-valid hallucination remains possible: deterministic validation and user confirmation are both required.

### 6.4 Feedback and discovery gate

Store feedback against opportunity revision. Not repetitive splits/suppresses the pattern; Wrong steps requests correction; Not worth it reduces relevance; Later snoozes thirty days; Never from this app creates a distinct suggestion policy, not an unannounced capture exclusion. Ignored cards stay unknown. Failed builds reduce capability confidence; successful real runs refine savings.

Do not ship ambient Discoveries on the owner's anecdotal success alone. Gate displayed precision at 85%, eligible-family recall at 60%, duplicate rate at most 5%, and at most one false suggestion/user-week on held-out data. Require the denominators in section 9. Failure preserves Teach/Tell and leaves mining in shadow mode; it does not justify lowering privacy defaults.

## 7. Micro-apps: SDK, manifests, execution, and repair

### 7.1 Artifact and SDK contract

A tool is an immutable published source tree plus canonical JSON manifest, independent test receipt, and approval signature. Each build uses a git staging repo; each approved version has a content hash. Real fixtures and run data live outside the repo. The active-version database pointer, not an editable symlink inside generated source, selects execution.

After runtime admission, generate TypeScript with ordinary pure functions and a small SDK. Reject a general graph DSL because its expression system/escape hatches would become a second programming language; the contingency in §7.3 is instead a finite host-owned CSV mapping with no expressions. Reject full native scripts because they bypass the effect boundary. TypeScript is an authoring choice, not a sandbox. The initial SDK is deliberately small:

```ts
const input = await files.readSelected("input_csv", {maxBytes: 10_000_000});
const rows = table.parseCsv(input, {required: ["order_id", "created", "amount"]});
const result = rows.map(r => ({
  Order: r.order_id,
  Date: date.toISO(r.created, {sourceFormat: "MM/DD/YY"}),
  Amount: money.decimal(r.amount, {currency: "USD"})
}));
assert.equal(result.length, rows.length);
await files.createOutput("output_root", "orders_ready.csv", table.toCsv(result));
return {rows: result.length};
```

Initial effects are selected-CSV reads and staging one create-only CSV output, with host-rendered parameter forms. Clipboard, notifications, moves, and additional families wait. CSV/date/money/text helpers are pinned host-owned library code; decimal amounts must not silently become binary floating-point money. No eval, dynamic imports, arbitrary dependencies, subprocess, HTTP, AppleScript, PowerShell, or UI control. Add PDF/image/API recipes only as fixed app-owned operations after their own tests. SDK limits are enforced by the broker even if generated code bypasses the wrapper. `files.createOutput` records staged bytes, never grants a live write.

### 7.2 Canonical manifest example

This is valid illustrative JSON; hashes/version pins are filled by the host after packaging. Owner-specific paths are resolved through native-picked handles stored in grants, not inserted into source.

```json
{
  "schema": "macroscope.tool/v1",
  "id": "tool_clean_import",
  "name": "Clean import CSV",
  "version": 1,
  "runtime": {"engine": "deno", "pin": "release-runtime-1", "sdk": "1.0", "entry": "src/main.ts"},
  "platforms": ["macos"],
  "inputs": {
    "type": "object",
    "additionalProperties": false,
    "required": ["input_csv", "output_root"],
    "properties": {
      "input_csv": {"type": "string", "format": "selected-file-handle"},
      "output_root": {"type": "string", "format": "selected-root-handle"}
    }
  },
  "triggers": [{"type": "manual"}],
  "capabilities": {
    "fileRead": [{"handle": "input_csv", "maxBytes": 10000000}],
    "fileCreate": [{"root": "output_root", "namePattern": "*_ready.csv", "overwrite": false}],
    "fileMove": [], "fileTrash": [], "network": [],
    "clipboard": "none", "nativeApps": [], "secrets": [], "modelCalls": false
  },
  "limits": {"wallSeconds": 120, "memoryMiB": 256, "effects": 1, "outputBytes": 20000000},
  "execution": {"initialTrust": "supervised", "concurrency": 1},
  "tests": {"suiteRef": "locked-suite-7", "fixtureDigest": "sha256:fixture-set"},
  "provenance": {"contractHash": "sha256:contract-v3", "buildRef": "build-7", "model": "muse-wonder-1.0"},
  "integrity": {"treeHash": "sha256:published-tree", "approvalRef": "approval-7"}
}
```

JSON Schema `format` names above are app-defined validators, not built-in filesystem authority. The host recomputes required capability union, file hashes, limits, and risk from actual artifact/plan checks. Approval signs a canonical digest excluding the signature itself; it binds source, dependencies, runtime/SDK pin, manifest, tests, and grant revision. Any change invalidates execution until reviewed. A software device signing key in the OS vault is sufficient initially; hardware-backed keys are optional, not an untested MVP dependency or malware-proof guarantee.

### 7.3 Runtime and effect protocol

**Runtime admission comes before a general SDK.** Choose a checksum-verified Deno release and one macOS launch: a signed launcher invoking a Seatbelt wrapper with a host-owned deny-by-default profile active before module loading. This is a proposed developer spike, not demonstrated confinement. M0 must produce a runtime lock artifact containing the actual Deno version/download digest, exact argv/environment, launcher/helper signatures and entitlements, OS versions, full sandbox profile/hash, allowed code/cache/pipe paths, aggregate limit enforcement, and denial receipts. `release-runtime-1` in §7.2 is a placeholder, not a pin. No generated execution outside the synthetic developer experiment until this artifact passes. If Seatbelt compatibility/support fails, choose the fixed host-owned transform representation below immediately; do not accumulate a second Mac sandbox project in the first product.

Use cached/local modules, a host-owned frozen configuration and lockfile, no permission prompts, and deny network/env/run/FFI/system access. Exact switches must be recorded from the pinned binary, not guessed here. The module loader can load the initial statically analyzable import graph without ordinary read/net permissions, and storage/cache APIs can consume disk without ordinary write grants. [VERIFIED-docs: [Deno security](https://docs.deno.com/runtime/fundamentals/security/), [permissions](https://docs.deno.com/runtime/reference/permissions/).] Therefore resolve the entire graph under the active OS boundary, canonicalize every file, and permit only hashed immutable generated source and SDK modules. Reject external file URLs, symlinks, private-fixture JSON imports, npm/jsr/native addons, remote/data/blob imports, generated config/task/hooks, and dynamic imports. No dependency downloads during validation or execution. Copy admitted bytes into an immutable, non-symlinked package so post-validation edits cannot swap modules. Run graph/type validation and execution under the same boundary; the privileged host never imports generated code. Static checks supplement runtime enforcement and are not a sandbox proof.

Test canaries for import-time private reads/network, symlink swaps, config discovery, worker/cache/localStorage/IndexedDB disk use, spawn/FFI/env, CPU loops, output floods, and descendant memory growth. Enforce **process-tree** resident memory (256 MiB initially), CPU time (30 s), wall time (120 s), one output ≤20 MB, logs ≤1 MiB/run, and per-run scratch/cache ≤32 MiB with external monitoring/quota enforcement and kill/reap. V8 heap flags are not aggregate memory limits; best-effort polling that permits unbounded growth does not pass. Broker user-data reads arrive over inherited pipes, and all writable runtime state is disposable, bounded, and key-free. Record OS limitations and reduce or block execution if these limits cannot be enforced.

Windows separately needs AppContainer token/profile/capabilities/ACL setup plus Job Object limits and broker-pipe tests; MSIX identity or a restricted token alone is not isolation. A signed child also does not automatically inherit an unsandboxed Mac parent's confinement; any future App Sandbox helper must demonstrate explicit entitlements/inheritance rather than rely on signing. [VERIFIED-docs: [Apple sandbox inheritance](https://developer.apple.com/library/archive/documentation/Miscellaneous/Reference/EntitlementKeyReference/Chapters/EnablingAppSandbox.html), [Microsoft AppContainer launch](https://learn.microsoft.com/en-us/windows/win32/secauthz/implementing-an-appcontainer).]

**Bounded contingency:** if Deno/outer limits fail, generate only a fixed host-owned CSV mapping representation: selected column names, fixed date formats, decimal currency parsing/rounding, required fields, and stop-on-error policy. No embedded expressions, scripts, user functions, or escape hatches. The host validates/interprets this finite configuration and uses the same preview/commit broker. This narrows expressiveness rather than weakening the boundary; evaluate it under the same one-family utility tests and identify its engine in the manifest. WASI remains later research, not an automatic claim of solved confinement.

Broker checks each request against tool/version/run identity, stage/commit phase, current grant and policy revisions, resolved handle, byte/action limit, and exact capability. Reject traversal, symlinks, root escapes, device files, and collisions. Generate filenames as data, never shell syntax. Escape spreadsheet formula-looking values according to the user's confirmed import contract. A manifest name pattern is only a ceiling; it does not approve arbitrary future contents.

Use replay for private fixtures and **stage once → review → trusted commit** for each real input. Generated code reads the selected snapshot and produces bytes into private encrypted staging, then exits and is reaped **before approval**. Preview renders those same staged bytes. The broker commits the frozen bytes without rerunning generated code and without another generated-code callback. There is no generated `live` mode. Any rerun, changed input, code/runtime/SDK, destination, limit, or grant revision needs a fresh stage and approval.

Define `planHash = SHA-256(canonical JSON plan)`, with host-defined field ordering/encoding and ordered effects. The plan binds source/version, runtime/SDK/dependency hashes, input snapshot digests and source identity/preconditions, resolved destination parent/volume identity and basename, ordered create effects with staged byte lengths/digests, aggregate limits, and grant/policy revision. User approval signs that digest; artifact approval and per-run effect approval are separate. Staging becomes read-only after hashing, and trusted commit rehashes the bytes. Immediately before commit, reopen/verify native handles without following symlinks, recheck identities, input preconditions, destination absence, and revocation. Use atomic no-overwrite publication of the approved bytes; if the filesystem cannot provide the required semantics, refuse that destination rather than emulate with a racy existence check. Journal intent and result; a crash between publication/receipt is reconciled by identity/digest and never blindly retried. MVP has exactly one create effect/run, one active run/tool, and two globally.

Adversarial acceptance includes a program that varies output by mode/time/randomness/input size: approval must commit the reviewed digest even if another execution would differ. Also test staging tamper, input/destination swaps, revocation before commit, and crash after publication. Later rename/move batches approve a frozen source→destination/identity list and ordered journal with collision checks; stop on partial failure and offer only conflict-checked recovery for completed entries. No batch enters MVP without that evidence. Timeout/denial kills the entire process tree, pauses the tool, and disables queued triggers.

### 7.4 Tests, undo, trust, and repair

Host-owned tests/oracles reside outside both agent workspace and generated-worker import/read allowlist. The validator supplies bounded fixture bytes through the broker and compares results in trusted code; do not mount hidden tests where generated code can read them. Include normal input, empty/missing fields, ambiguous/invalid dates, collision, source preservation, permission denial, renderer payloads, and output invariants. Synthetic examples validate shape; user-approved local input/output pairs validate intended business rules. Observation alone does not supply a golden output.

Require schema/static checks, independent hidden tests, fixture plan comparison, capability-ceiling check, and staged user comparison before pinning. At most four build-feedback iterations within the same budget/time ceiling. Agent-authored tests help diagnosis but cannot certify installation. Exit zero or Muse's “done” does not pass this gate.

Journal effect intent, target token, staged/committed hashes, and completion receipt. MVP preserves originals and refuses overwrite; it links the created output rather than implementing general undo. Later create undo moves output to Trash only if identity/digest still match; overwrite/move undo verifies no intervening change before restoring encrypted preimages. Retain later undo seven days. No universal undo claim for clipboard changes or external services; only expose an inverse when current preconditions hold.

Trust starts supervised, and each new MVP input requires staged-byte review. Later, three clean runs may offer fewer navigation prompts within the same enforced scope, never safety certification or a waiver of output approval. Future unattended folder triggers require a separate authority/effect-approval design plus standing grants and readiness/debounce/idempotency checks; context suggestions never auto-run. Any code/permission change resets trust. Three ordinary failures trip a circuit breaker; any sandbox denial stops immediately.

Classify failures before repair: auth→replace key; quota/network→bounded wait; input validation→choose/correct mapping; permission→review grant; code/data drift→new reviewed build initially. The later repair UI uses a new branch/workspace/session, a reviewed synthetic packet, and a host-owned regression reproducing the fault. At most two repair candidates within the total reservation; original/new tests must pass, permissions cannot widen, and the user approves the new version. No automatic patch deployment or uncertain-effect replay. Rollback selects an earlier valid signed version; it does not undo effects already committed.

## 8. Privacy and security

### 8.1 Threat model and non-promises

Protect capture evidence, third-party information, API keys, tool integrity, grants, fixtures, and run preimages. Attackers include malicious page/document authors, faulty model output, generated programs, compromised dependencies/updates, physical viewers, local malware, network attackers, and abusive monitoring users. Same-user malware or an administrator may defeat a user-mode application; encryption and process separation reduce easy leakage without establishing enclave-level protection.

The dangerous cross-boundary path is hostile screen text → invented task → generated code → privileged effect. Break authority at every hop. Avoid employer dashboards, stealth recording, remote observation, hidden icons, and audio. Do not infer emotions or profile individuals. Privacy/legal eligibility of real workplace use and international distribution remains UNVERIFIED; BYOK and no audio do not themselves settle those questions.

### 8.2 Prompt-injection defenses

1. Mark all page text, filenames, labels, OCR, fixtures, tool output, and model prose untrusted; only deliberate user correction is instruction.
2. Keep the analyzer tool-free and schema-constrained, with bounded enums and evidence references; delimiters alone are not a defense.
3. Deterministically reject unknown evidence/capabilities, sensitive paths, new destinations, and unsupported claims; require native user selection for actual roots/origins.
4. Convert the accepted contract through a host template into a synthetic build packet; do not pass raw recordings or unrestricted analyzer prose into instructions.
5. Give Muse an untrusted synthetic workspace, minimal personal context, restricted tools, and no shell; test read/egress confinement rather than assuming flags cover every tool.
6. Independently test and sign the artifact; enforce every effect in the broker at runtime. Generated code cannot grant itself new access.
7. Treat repaired source and test feedback as equally untrusted. No repair bypasses consent, regression, permission diff, or publication gates.

A local injection classifier may flag suspicious content later; Prompt Guard is not an MVP dependency because the plan does not require another model invocation or rely on classifier accuracy. Static scans provide diagnostics, never proof that arbitrary TypeScript cannot hide an effect.

### 8.3 Ship blockers by admitted feature

All requirements apply before the named surface ships. Capture is removed from MVP, so P1/P2 and capture-specific storage/evaluation are deferred with the feature; security of shipped data and execution is not deferred.

| ID | Required implementation/evidence | Admission |
|---|---|---|
| P1 | No raw pixels/OCR/AX content on disk, IPC, logs, or crash attachments; sanitize before persistence/egress; fail closed on detector failure. | Before capture |
| P2 | Allowlisted scope, secure-field/lock/pause gating, visible capture state, barrier race tests, browser exclusion without heuristics fallback. | Before capture |
| P3 | SQLCipher metadata and authenticated encrypted fixtures/staging, device-vault keys, no plaintext temporary fallback; encrypted shards/preimages when those features enter. | MVP; extend per feature |
| P4 | Lineage-aware fixture/build/run deletion, cancellation/reap barrier, key destruction/rekey and backup exclusions; images 24 h/detail 7 d/aggregates 30 d when captured. | MVP; capture retention later |
| P5 | Separate egress consent, payload preview, exact direct-request log/honest Muse-input log, Local-only active-job barrier, Contributor ban. | MVP |
| P6 | Own credential item; no shared auth mutation or key in argv/prompts/PTY/runtime; secure harness, successful API billing and rejected-key/no-fallback tests. | Before paid inference |
| P7 | No arbitrary keyed shell; proved native Muse tool/read/egress containment or zero-tool direct route; generated runtime has no model key. | Selected builder before use |
| P8 | Locked host tests, immutable versions and run-byte approval, scoped grants, brokered effects, pinned loader/OS confinement and aggregate limits, kill switch. | Before generated execution |
| P9 | Signed/notarized app/helpers, verified runtime downloads, no silent dependency installs, fresh signed install/manual N-1 update; signed updater before automated updates. | External MVP; updater later |
| P10 | No telemetry by default or memory dumps, scrubbed bounded logs, synthetic-only diagnostics; privacy policy before external testers. | MVP |
| P11 | Escaped/sanitized renderers, isolated zero-IPC previews, CSP/resource/navigation denial, malicious-content fixtures; clipboard/link escape controls before terminal. | Every shipped renderer |

Default denylists supplement explicit allowlists and cannot make collection universally safe. Redaction uses structural masks, credential/PII detectors, local OCR bounds, and opaque fills; drop uncertain frames. Do not add contact-book access or promise names/health data are always recognized. Retention starts at capture, not pinning, so abandoned drafts expire too. User-approved samples have an explicit separate expiry; keep synthetic regression shapes after sample deletion, with reduced validation coverage disclosed.

Outbound exact payload bodies expire after seven days; metadata/cost rows after thirty, with optional content-free monthly totals. Synthetic Muse build sessions expire after thirty days or earlier user deletion. Approved source remains until deleted. Presence-gated evidence viewing is desirable before public launch but must not be described as protection from same-user malware.

Standard-mode training commitments are distinct from retention. The current terms describe retention without a fixed universal deletion period; `store:false` is not a zero-retention promise. [VERIFIED-docs: [Meta terms](https://dev.meta.ai/legal/terms-of-service), as rechecked; legal applicability UNVERIFIED.] Review terms/embedding rights and end-user disclosure with appropriate expertise before public release.

### 8.4 Renderer and document boundary

Treat filenames, CSV cells, field values, manifest/parameter labels, source/diffs, logs, transcript text, and generated outputs as untrusted. Text-escape them by default; render Markdown with raw HTML disabled, safe link schemes, no remote images, and explicit user confirmation for external links. Use a maintained allowlist sanitizer only where markup is necessary. Host-render parameter forms from validated schema without generated HTML/JS, and later invoices from escaped data in host-owned templates. A CSV preview is a host-owned table, never HTML interpreted from cells.

Any later HTML preview uses a dedicated unprivileged webview/origin with **zero product Tauri IPC capabilities**, no scripts, remote/local resource fetching, forms, navigation, popups, or downloads. Use an isolated in-memory document and minimal CSP such as `default-src 'none'; script-src 'none'; connect-src 'none'; img-src 'none'; style-src 'none'; font-src 'none'; object-src 'none'; frame-src 'none'; base-uri 'none'; form-action 'none'`; styling, if needed, is fixed host content authorized by exact hash, never user CSS. Enforce navigation/resource denial in the host too. Product capabilities name only trusted product webviews; no wildcard window grant includes previews. [VERIFIED-docs: [Tauri capability boundary](https://v2.tauri.app/reference/acl/capability/); proposed policy requires packaged tests.] Deno network denial does not constrain a browser renderer.

Exported HTML drafts must remain safe when opened externally: fixed self-contained template, escaped fields, no scripts, event handlers, external URLs/resources, active links/forms, or arbitrary user markup. A preview sandbox cannot make a dangerous exported document safe. Terminal, if later added, disables OSC clipboard writes and automatic URL opening and has no product IPC. Test script/event payloads, `javascript:` links, external images, SVG/CSS URLs, malicious labels/diff headers, transcript Markdown, and terminal escapes across every shipped surface; assert zero network requests, navigation, clipboard changes, or privileged command dispatch. Restore these requirements from [research 05 §2](research/05-alt-builders.md#2-muse-tui-in-an-embedded-pty) and [research 11 §6](research/11-architecture-codex.md).

## 9. Cost/performance budgets, evaluations, and release gates

### 9.1 One operating cost model

Use Standard prices, no assumed cache savings: Spark input/cached/output $1.25/$0.15/$4.25 per million; Wonder $4/$0.20/$20. [VERIFIED-docs: [Meta pricing](https://dev.meta.ai/docs/pricing-rate-limits).] Rates are a dated baseline; store a versioned price table and recheck before releases. Aggregate billable output already including reasoning must not have reasoning added twice.

| Workload assumption | Calculation | Cost |
|---|---|---:|
| Contract interpretation | Spark 6k input + 2k output | $0.016/task |
| Complete bounded build | Wonder aggregate 200k input + 20k output | $1.20/build |
| Complete repair | Wonder 80k input + 8k output | $0.48/repair |
| Private baseline: 8 builds + 2 repairs/month | 8 × $1.216 + 2 × $0.48 | $10.69/month |
| Later sparse discovery maximum typical admission | 5 packets/day × (4k Spark input + 1k output) × 22 days | $1.02/month |
| Public-v1 central total | Baseline + sparse discovery | $11.71/month |
| Planning reserve | Unrounded central total × 1.25 | $14.63/month |

These are workload estimates, not measured Muse costs. Reminder/child calls, repeated context, failed attempts, and missing usage can exceed them. The $19/month proposal analyzes every episode; the $43 option uploads many images; the $32 model assumes daily/nightly/weekly analysis. Those are rejected workloads, not arithmetic alternatives for the same design. No-cache heavy scenario—16 builds at $4 plus four repairs at $1.50—already exceeds $70 before observation.

Default production admission limits: $25/month builds/repairs, $3/month analysis, $0.20/day background analysis (later), $2/build, **$0.75 total per repair job across at most two candidates** (later repair), ten-minute build wall time, one paid job globally. Production and repair eval use the same reservation ceiling; exhausted attempts and measured spend overruns count as failures. Successful repairs must satisfy the measured $0.75 total threshold; an admission limit does not guarantee final provider charges. Explicit continuation creates a separately reported policy cohort/reservation, never silently raises the gate. Keep conservative reserves for in-flight work; local accounting cannot guarantee a strict provider dollar ceiling. The monthly table is a workload hypothesis including later repair/discovery, not observed typical MVP spend. Signing, CLI/tool fees, Windows test infrastructure, and support are separate costs.

Deduplicate ledger legs by stable identity, reconcile cumulative deltas, and account for observed reminders/children separately without double-counting parent totals. User-created subagents/goals are disabled. If unavoidable reminder usage cannot be measured, reserve for it and block wider beta until console reconciliation is within 5% over a pilot week. Null catalog cost is not zero. Keep unknown usage reserved after transport loss; never automatically resubmit potentially billed work.

### 9.2 Performance contract

Target aggregate background footprint below 350 MiB, peak capture below 700 MiB, native observer below 200 MiB, and no model host after ten idle minutes. Quiescent CPU below 0.5% of one core; active capture path below 3%; total background processing below 5%. One-GiB evidence quota and three-GiB disposable cache cap exclude user-owned source. Bound logs at fifty MiB and output per manifest.

Latency targets: pause p95 250 ms; buffer disposal one second; UI progress within two seconds; Task Contract proposal within sixty seconds after a bounded description (later also demonstration); p90 testable build within ten minutes. Target incremental background battery drain at most three percentage points of usable capacity/eight hours, measured separately from intentional builds. All are UNVERIFIED measurements.

Profile release builds with matched app-off/app-on workloads, attributed child/WindowServer costs, eight-hour leak soak, and ABBA battery runs. Fix brightness, displays, thermal state, and workload seed. Measure physical footprint/Wh rather than comparing unlike RSS or Energy Impact values. Windows needs ETW/WPR traces and physical testing; a hosted-runner synthetic pass is not battery evidence.

### 9.3 Evals and release gates

Freeze corpus, adapter/prompt/model/runtime versions, price table, permissions, and acceptance tests in an eval manifest. Separate synthetic robustness from natural utility; split by workflow family and recording day, keeping all perturbations together. Separate inexpensive deterministic renderer/broker/protocol regressions from fresh paid generation/repair evaluation; replayed/cached model output tests scoring only, not generation quality. Paid ceilings are funding gates, not presumed spend authorization.

At the planning rates, 30 tasks × 3 fresh generations × ($1.20 + $0.016) = **$109.44**. One fresh contract per task instead is $108.48. Forty later repair faults × $0.48 = $19.20, plus twenty healthy-control calls × $0.48 = $9.60: **$138.24** combined baseline, **$172.80** with 25% reserve. Propose a $175 total evidence reservation for that combined scope, keeping $50 maximum per paid batch and a single frozen eval manifest/policy across batches. The initial build-only suite needs $136.80 with reserve; it is not funded by the $10 spike allowance. Record full totals across batches, retries/reminders/children, and non-API infrastructure separately. Actual measured costs can require a larger explicit budget or leave the gate incomplete; a stopped suite never claims the missing denominator. Replace assumed token counts with per-attempt distributions after the first batch.

| Gate | Required result and denominator |
|---|---|
| Privacy, every release | Zero high-risk canary leaks/unauthorized effects on shipped retained/outbound/log/render surfaces; Local-only/Delete active-job barrier and staged-byte adversarial cases pass. Before capture: ≥3,000 distinct high-risk placements, ≥99.5% entity recall overall and ≥99% per adequately populated category. |
| Capture/state, before capture ships | No content after acknowledged pause/lock/exclusion; focus/policy race, picker versus broader permission, revocation, crash, update, and disk-full tests; bounded queue/storage. Not an M0 full-suite requirement. |
| Private generated execution | ≥75% bounded hidden-test success across ≥30 distinct local tasks, three repetitions/task; zero forbidden effects; all accepted requests counted, infrastructure failures labeled separately. |
| Discovery/public v1 | ≥85% displayed precision, ≥60% eligible-family recall, ≤5% duplicates, ≤1 false card/user-week; ≥50 judged suggestions and ≥30 positive families, real/synthetic scores separated. |
| Repair, before repair ships | ≥80% executable success on ≥40 injected faults, ≤5% needless changes on ≥20 healthy controls; zero permission expansion; median ≤$0.50, p90 ≤$0.75 total within two candidates; budget exhaustion counts as failure. |
| First-use comprehension, before beta | Five unfamiliar users (≥2 non-developers); ≥4 second-input runs ≤30 min with prerequisites; all identify read/write/cloud scope and stop build; zero accidental grants; fresh-account cohort/setup failures reported separately. |
| Wider beta retention | ≥20 matured real requests; ≥50% test-passing and retained at 14 days; ≥70% retained among passing tools; no synthetic substitutes. |
| Distribution/platform | Fresh signed install, N-1 update, helper identity/permissions, emergency stop, sandbox escapes, usage reconciliation, and resource budgets pass on each claimed platform. |

Report uncertainty/denominators, not a blanket “safe” badge. Private generated-execution gate applies to the one admitted CSV family; discovery, capture and repair gates apply before those later features, not before the Tell-only product. Synthetic developer M0 has denied-effect/hand-checked evidence, not the 90-build release qualification. A newly found high-risk leak blocks release regardless of averages. If quality misses, narrow supported tasks; if discovery misses, keep it shadow-only. Budget exhaustion, crashes, and failed builds remain in the all-request denominator. A passing tool created at the end of week two has not completed its 14-day retention follow-up.

## 10. Roadmap: macOS first, then Windows

The order below is sequential solo capacity. Ranges are optimistic hypotheses, not commitments; log actual engineering/support hours in M0/M1 and publish a revised forecast before expanding scope. Gates override dates. Keep Windows compile/schema checks early; packaged interactive evidence comes before Windows delivery dates. Capture has a separate synthetic experiment and never blocks Tell by forcing a capture archive into M1.

| Milestone | Estimate | Deliverable and exit condition |
|---|---|---|
| M0 | Weeks 1–2, timeboxed | Safe key harness; one API-billed direct call; MSP success or precise profile/confinement fallback; one pinned runtime denial experiment; trusted CSV staged-byte commit. Separate signed synthetic capture feasibility log, timeboxed without delaying Tell. Record every gate passed/failed/unknown. |
| M1 | Weeks 3–6 hypothesis | One selected CSV → one new CSV; vault, minimal encrypted task/fixture/job/grant storage and deletion barrier, host tests, immutable versions, stage/compare/commit and receipt. Runtime/route decision artifacts precede dependent UI; no general SDK before runtime evidence. |
| M2 | Weeks 7–10 hypothesis | Tell/sample/setup failure UX, save/manual rerun on second input, renderer/egress/crash tests, only the selected builder's protocol subset/recovery. No terminal, Show, clipboard, move/undo, or repair system. Measure engineering time and reforecast. |
| M3 | Weeks 11–14 hypothesis | Owner dogfood, funded 30-task × 3 fresh-build suite for the one family, usage/resource reconciliation, signed fresh install/manual N-1 update; owner alpha only after shipped-surface security gates. No claim of matured retention yet. |
| M4 | Weeks 15–22 hypothesis | Five-user formative gate, then 5–10 technical BYOK Mac testers; mature ≥20 real requests through 14-day follow-up, count setup/support effort; private-beta/public-name/legal decisions. Expand no families until value beats existing-agent baseline. |
| M5 | After M4; separately estimated | First choose the highest-value increment: another family/reviewed repair OR Show/capture storage. Discovery first needs two-task/negative-control provenance evidence, then shadow evaluation. Chromium and unattended trigger are independent projects. Run funded Windows packaged vertical spike (§5.4) before assigning M6/M7 dates. |
| M6 | Re-estimate only after Windows spike; reserve ≥4–8 weeks as hypothesis | Windows 11 x64 MSIX activation/update, vault/selected-builder/runtime/broker tests, interactive evidence and physical tester. Tell can enter closed beta before Windows capture; WGC/UIA/OCR requires its own redaction/permission gates. |
| M7 | After M6; no calendar promise | Windows soak/update/security/retention gates; supported release with explicit capability matrix. Each added capture/browser/trigger feature passes its own evidence gate; ARM64 and native/API recipes remain later. |

If MSP admission or containment fails during M0, select direct zero-tool Meta generation immediately; if generated execution fails, narrow to the fixed host-owned CSV representation. Spend at most one additional week proving the selected contingency. If neither route works, keep only trusted local experiments and record the blocker. Do not begin broader UX/library work around unresolved boundaries or silently drop outer confinement.

### First two weeks: concrete daily work

| Day | Work and reviewable output |
|---|---|
| 1 | Check Rust/Tauri/Swift/signing availability and exact Muse binary/schema; freeze one fabricated orders task, independent expected CSV/negative cases, canaries; minimal Mac shell plus Windows core/schema compile. |
| 2 | Dedicated private-input smoke harness: allowlisted env, denied approvals, exact turn/session matching, bounded scrubbed logs; prove no secret argv/logs with bogus canaries. Native key-entry/vault seam, without touching shared Muse auth. |
| 3 | Echo/bogus-key profile admission tests; reproduce/classify probe 21, discover only supported posture, test `promptUnmatched` separately. Record actual profile/config and private-HOME/native read/web/plugin/child denied effects. Failed posture/confinement selects direct route. |
| 4 | Freeze direct Responses `text.format` synthetic request/response/error fixture design; compare public SDK effort with minimal MSP adapter; finish any no-key containment test needed before a paid spawn. |
| 5 | If owner supplies dedicated key/budget: one tiny direct Spark request; one exact-session/turn serve Spark call only if profile/confinement passed. Record usage/API-dashboard attribution and rejected-app-key/no-fallback control. Otherwise label paid gates open and continue synthetic work. |
| 6 | Pin actual Deno version/argv/checksum and Mac Seatbelt launcher/profile; run malicious loader/read/network/cache/memory/process denial fixtures. Record limits/entitlements. If it fails, select finite host-owned CSV configuration, not another unbounded runtime project. |
| 7 | Trusted handwritten CSV converter exercises stage once → escaped host comparison → frozen plan/digest → atomic no-overwrite broker commit → receipt. Test input/target swaps and crash reconciliation before generating code. |
| 8 | If paid/runtime gates pass, one synthetic Meta-generated candidate via selected route, hidden fixture checks and immutable version; otherwise exercise trusted converter and record missing gates. Test differing-output adversary and staging tamper. |
| 9 | Separate timeboxed signed Swift/SCK/AX experiment: selected synthetic window, picker versus broader access, helper attribution/grant/revoke/pause and no persistent raw content. Record failures without coupling capture storage or discovery to Tell. |
| 10 | Evidence report: exact versions/commands/profiles, denied effects, billing status, signed-shell install evidence if available, initial resource sample, actual effort and revised forecast. List unrun tests explicitly; no full updater, repair UI, 90-build evaluation, or retention claim. |

The **Tell end-to-end spike** uses one fabricated orders CSV, confirmed MM/DD/YY/date/decimal rules, and independently authored expected output. Spark interprets the reviewed synthetic packet through Bearer auth. The selected builder uses the same app-owned API key: Muse only in child `META_API_KEY` after profile/confinement passes, or direct zero-tool Wonder generation. The host validates a candidate outside the agent workspace; admitted Deno or the fixed host representation stages selected-input output once, and the broker commits the exact approved bytes. A handwritten-converter result is labeled separately from generated-tool success. Capture feasibility is a separate artifact, not evidence that Tell inferred a workflow.

Day 5 proves only transport/auth/accounting for the route actually exercised. It does not prove generation, containment, or a finished tool. Proposed allowances are at most $1 for tiny auth calls and a separate $10 week-two generation experiment, requiring an owner-supplied key and budget; this planning revision spends nothing and grants no credential access. Missing key/profile/runtime/signing prerequisites leave those gates open, not presumed passed. M0 outputs are an evidence report and bounded experiments, not an external release.

Collect consented personal CSV examples and synthetic oracles immediately; label rules, manual explanation/review/active time, errors, unsupported requests, and the existing-agent comparison. Add episode/coverage labels only in the separate observation study. Keep reusable exports synthetic. Retention studies start when tools exist. Run trusted interactive platform jobs in a dedicated test account; never expose production signing/API secrets to untrusted PR jobs or an unrestricted self-hosted runner.

Release candidates compile locked dependencies, verify nested binaries, sign/notarize/staple Mac artifacts, sign Windows packages, and test actual downloaded/quarantined bytes. First private installs can update through a verified signed manual replacement and N-1 migration; an automated updater is deferred, not simulated in M0. Publish signed automated-update payloads only after updater/N-1/permission checks. Promote the same tested bytes; do not rebuild for stable. Installation waits for active jobs, and no urgent updater silently replaces a running tool.

## 11. Ranked risks and kill criteria

Kill criteria stop the named feature or launch; they do not authorize silently relaxing the boundary.

| Rank | Risk | Early evidence and kill/pivot criterion |
|---|---|---|
| 1 | Injection or secret leakage | Any high-risk canary in persisted/outbound artifacts or unauthorized runtime effect blocks all external release; unresolved containment moves to zero-tool generation/pure transforms. |
| 2 | Meta auth/billing/terms | No safe API-key-only MSP path immediately selects direct zero-tool Meta generation; no successful permitted Meta route after contingency week blocks paid generation. Never use consumer login as workaround. |
| 3 | Key-bearing Muse tools | Arbitrary shell/read/egress route remains available with real key: disable that integration; terminal convenience cannot waive the gate. |
| 4 | Generated-tool reliability | Below 75% bounded success after M3 tuning: narrow templates; below 40% first-candidate success by pilot day 60: pivot to explicit Tell/template tools. |
| 5 | Permission/distribution instability | Signed helper cannot preserve reliable consent/update behavior, or requires hidden/privileged capture: drop affected capture mode; Tell remains viable. |
| 6 | Weak sustained value | Under 25% 14-day survival or median savings fail to exceed setup/repair after 20 matured tasks: revisit workflow/persona before expanding discovery. |
| 7 | Discovery cannot infer useful rules | Allowed-signal tasks indistinguishable from negative controls: postpone mining. Later precision <85% or eligible recall <60% blocks ambient rollout; do not broaden capture to rescue metrics. |
| 8 | Maintenance/cost treadmill | Median successful build >$2 or >1 repair/tool/month across two pilot cohorts: narrow task types; do not subsidize silent retries. |
| 9 | Windows unverifiable/EDR blocked | No packaged chain/interactive/physical evidence, or operation requires disabled security/elevated observer: Windows stays preview or drops affected capability. Documented one-time Muse sandbox UAC setup is separately reviewed/tested, never permission for an elevated recorder. |
| 10 | Solo scope/platform churn | Two milestones slip >50% due to integrations or repeated Muse breakage: freeze new adapters; use bounded direct artifact path if needed. |
| 11 | Platform absorption/name conflict | Users prefer existing agents and refuse to retain tools: test a bookkeeping/export vertical; public naming clearance required before public marketing. |

## 12. Open questions for the owner

These are the remaining owner decisions, not requests to reopen the selected architecture. Recommended answers are implementation defaults until a different answer is supplied; external prerequisites remain gates rather than presumed approvals.

| Priority | Deduplicated owner question | Recommended answer |
|---|---|---|
| P0 | Which one recurring CSV task and expected output can define personal value? | Orders export-to-import; confirm date/decimal rules; fabricated data first and local-only real fixtures. |
| P0 | Can a real Meta developer key with billing/model access be supplied through native secure entry for M0? | Yes; dedicated app key, tiny auth smoke test then $10 week-two experiment budget; never paste it into source/chat/argv. |
| P0 | Is the installed Muse account still healthy after the research incident, and does the overnight keeper need it? | Check independently; repair personal CLI login outside Macroscope if needed; app must never alter that account. |
| P0 | What signing identity/bundle identifier will persist, and who owns publication rights? | Fix an owner-controlled identity before alpha and keep it across updates; obtain redistribution/embedding review without bundling Muse. |
| P0 | Is Tell → one CSV family acceptable, with capture/MSP as gated evidence tracks and direct Meta fallback if MSP fails? | Planning default: yes; keep API-billed Muse models, deterministic retained tools, and staged-byte authority; defer terminal, capture and full repair. |
| P1 | May any work/employer data leave the device, and under what policy? | Personal/authorized work only; per-task cloud review initially; no employer deployment or legal-exemption assumptions. |
| P1 | What spend/storage defaults should launch with? | $25/month builds plus $3 analysis, $2/build, $0.75/repair, one-GiB evidence; increase only after visible measured use. |
| P1 | Is 24-hour image/7-day detail retention acceptable? | Yes; thirty-day coarse aggregates, explicitly approved fixture expiry, honest expired-evidence UI. |
| P1 | Who funds and validates Windows? | Budget a compatible interactive VM/cloud environment and an independent physical x64 tester before M6; no owner hand-testing dependency. |
| P1 | Can the full fresh-generation evaluation be funded beyond the $10 spike? | Before M3, explicit build-only reservation around $137; combined later build/repair/control evidence about $175 with reserve, measured retries/children may need more. $50 is a batch ceiling, not total funding. |
| P1 | Which browser and public name? | Chromium first; keep Macroscope internal until name/domain/trademark review; do not assume suggested alternatives are cleared. |
| P2 | Should pricing, gallery, auto-repair, runtime AI, or local models move earlier? | No; retained tools first, BYOK beta, explicit reviewed repair, deterministic runtime, all Meta inference API-billed. |
| P2 | Should capture/redaction be published for trust? | Publish interface, threat model, security behavior and synthetic tests first; decide code licensing separately before public launch. |

The first implementation ticket is the synthetic CSV task/oracle plus secure-key smoke harness and selected-route/runtime evidence in M0. Its deliverable is reviewable passed/failed/unknown evidence, not a finished capture-to-automation product. Signing/key/tester/budget availability remains an external prerequisite; a recommended answer above is not a grant to spend or access personal credentials.

### Decisions I made

- Keep the original three-stage product thesis, but validate retained Tell-first CSV value before capture/discovery investment.
- Separate MSP profile admission, API billing, keyed native-tool confinement, and generated-runtime confinement into evidence gates.
- Commit approved staged bytes through trusted code; never rerun generated code after output approval.
- Apply renderer isolation and active-job egress/deletion barriers to every shipped surface.
- Keep concrete later designs while removing them from the first private product and calendar promises.
- Preserve the monthly arithmetic; fund fresh evaluations separately and align repair admission/evaluation ceilings.
