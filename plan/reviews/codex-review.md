# Codex technical and UX review

Reviewed 23 October 2026 against [BRIEF](../BRIEF.md), [PLAN](../PLAN.md), the research reports, the exported MSP schema/help, and the recorded probes. Web checks below use primary documentation. **Local evidence** means inspection of supplied artifacts, not a rerun. **Inference** identifies my assessment or a proposed attack; it is not a demonstrated exploit. No paid calls, credential operations, or permission changes were performed. PLAN.md was not edited.

**Verdict: approve a smaller evidence spike; do not accept this as a credible solo delivery commitment yet.** The product boundary is substantially better than the earlier research: deterministic local outputs, deliberate teaching, independent validation, and no keyed shell. But the implementation still depends on two unproved execution boundaries, the latest probe has an unacknowledged startup failure, and the proposed private MVP includes too many systems before validating demand. The discovery promise also outruns the observation signals actually specified.

## The ten most important problems, ranked

### 1. API-key-only Muse operation is still blocked, and the proposed real-key probe is unsafe

**Severity: P0; blocks the paid MSP spike.**

**Evidence.** PLAN §§4.3–4.5 correctly treats successful inference/billing as unverified. However, [21-goodkey.jsonl](../probes/transcripts/21-goodkey.jsonl), omitted from the probe README, adds a concrete failure: lines 3 and 9 report an unavailable `:auto-review` permission profile; line 10 rejects `session/start` with `permission_profile_reviewer_unavailable`. There is no successful turn. Its initial record has no environment overrides. The [probe client](../probes/msp_probe.py), lines 35–39, removes inherited `META_API_KEY` before applying overrides, so this artifact does not establish use of the app's environment-key lane, even though its catalog came from the provider.

The suggested real-key command in [research 04 §0.2](../research/04-muse-msp.md#02-successful-call-with-a-real-key-not-run) uses `--env META_API_KEY=$KEY`. Shell expansion puts the secret in the Python process's argv. Redacting `env_overrides` in the transcript does not fix that exposure. The same harness defaults to approving requests, records raw frames/stderr, and accepts any turn terminal without matching the expected ID. It was suitable for bogus-key exploration, not an unchanged real-key security test.

There is a second boundary: [serve help](../reference/muse-cli-help.txt), lines 249–257, describes filesystem/network sandboxing specifically for shell execution. Shell-off plus restricted network does not establish confinement of native reads, web tools, plugins, or automatic reviewers. [Probe 18](../probes/transcripts/18-env-leak-usershell.jsonl) demonstrates environment-key exposure through userShell. The plan acknowledges this risk, but the missing implementation remains on the critical path.

**Concrete fix.** Before paid use, make a dedicated smoke harness with secure key input/private pipe, no secret argv, an allowlisted environment, deny-by-default approvals, bounded scrubbed logs, and exact session/turn matching. Select and record an available permission posture using supported configuration; explicitly test whether `promptUnmatched` avoids the recorded failure rather than assuming it does. The [schema](../reference/msp/msp.d.ts), lines 2296–2318, distinguishes approval mode from permission profile. Its comment mentions `permissionProfile/list`, but the exported method union does not establish a usable typed implementation of that method; verify the actual surface.

Then prove fresh private HOME + app key + explicit Standard model → successful turn → usage → API dashboard attribution. Repeat with an existing personal session and a rejected app key to exclude fallback. Separately prove native read/tool/egress containment. If it fails, activate the already named direct Meta, zero-tool artifact path immediately. Do not spend the UX budget building around an inaccessible MSP posture.

For the direct Spark leg, freeze a successful request/response fixture for `POST /v1/responses`, including schema enforcement, output limits and error handling. The Responses guide explicitly requires `text.format`; top-level `response_format` can be accepted without configuring structured output. The generic structured-output guide's examples use Chat Completions. Copying that example into the selected endpoint would silently weaken the contract. PLAN correctly leaves exact nesting as a live gate. [Verified: Responses compatibility notes](https://dev.meta.ai/docs/protocols/responses), [structured-output examples](https://dev.meta.ai/docs/structured-output).

### 2. The Deno runtime boundary is still a research project, with a concrete module-loader trap

**Severity: P0; blocks execution of untrusted generated code outside the developer experiment.**

**Evidence.** PLAN §7.3 has no pinned Deno command or working outer sandbox. It names a sandbox helper, Seatbelt, AppContainer, and WASI as successive possibilities. None has supplied probe evidence. [Research 10 §§4.1–4.4](../research/10-microapp-runtime.md) also leaves flags and hardened-runtime/JIT entitlements unverified. Calling all of this a day-seven task conceals several platform integration experiments.

Deno's documented permission model has an important exception: the initial statically analyzable module graph loads without normal read/network permission checks. Its reference also describes disk-consuming storage APIs that do not need ordinary read/write permission. Thus `--deny-read`/`--deny-net`, a lockfile, or a heap limit alone cannot establish the plan's claims. This is a documented attack surface, not a demonstrated escape in this repository. [Verified: Deno security](https://docs.deno.com/runtime/fundamentals/security/), [permission reference](https://docs.deno.com/runtime/reference/permissions/).

**Concrete fix.** Pin one runtime and demonstrate one macOS launch architecture before writing the general SDK. Resolve and validate the entire import graph against an immutable code/SDK allowlist; reject external file URLs, symlinks, JSON imports from private fixtures, npm/native addons, remote imports, and generated config/task hooks. Test imports during validation as well as execution. Keep the OS boundary active during loading. Cap process-tree memory, CPU, output, scratch, and runtime caches; a V8 heap setting is not an aggregate process limit.

Choose either a signed sandboxed helper or a tested Seatbelt wrapper, with concrete entitlements/profile and denied-effect artifacts. Apple's inheritance rules require deliberate helper configuration; a signed child is not automatically confined by an unsandboxed parent. [Verified: Apple sandbox inheritance](https://developer.apple.com/library/archive/documentation/Miscellaneous/Reference/EntitlementKeyReference/Chapters/EnablingAppSandbox.html). If this fails, narrow to a fixed host-owned transform representation or the proposed WASI contingency. Do not silently drop the outer boundary to preserve the schedule.

### 3. Approval is not explicitly bound to the bytes that will be committed

**Severity: P0; a trust-boundary gap, even with perfect filesystem isolation.**

**Evidence.** PLAN §7.3 specifies replay/plan/live runs, a plan hash, and input/output preconditions, but never defines the canonical plan or requires each live effect's payload to match its previewed digest. It expressly says that live results can differ. The manifest example, §7.2, authorizes creating any matching `*_ready.csv`; a content-hash signature identifies the program, not the correctness of every later output.

**Attack inference.** A generated program behaves correctly in tests and preview, then changes an amount or adds rows during live execution based on mode, time, randomness, or input size. It writes only the approved filename, within byte limits. All filesystem permission checks pass. Hidden tests reduce ordinary errors but cannot prove equivalence for arbitrary TypeScript. The user approved a comparison that was never actually committed.

**Concrete fix.** For v1 file/text transforms, execute once into staging. Present that exact output, then have trusted broker code commit those already staged bytes; do not rerun generated code after approval. Bind approval to version/runtime/SDK, input digests, resolved destination identity, ordered effects, output byte digests, limits, and grant revision. Revalidate preconditions immediately before commit using handles and atomic no-overwrite semantics. For rename batches, approve a frozen source→destination list and define partial-failure recovery. If rerunning later is necessary, create a new staged plan and approval. Add an adversarial fixture that deliberately emits different preview/live content.

### 4. Untrusted preview content can bypass the runtime boundary through the UI

**Severity: P0; missing security requirement.**

**Evidence.** PLAN includes HTML invoice output (§1.2), source/diff/transcript/output previews (§2.5), generated parameter forms (§7.1), and a PTY. It does not specify a CSP, HTML renderer policy, navigation/resource policy, or capability isolation for those surfaces. This loses explicit safeguards from [research 05 §2](../research/05-alt-builders.md#2-muse-tui-in-an-embedded-pty) and [research 11 §6](../research/11-architecture-codex.md): terminal clipboard/link controls and a separate unprivileged origin for generated content.

**Attack inference.** An invoice field contains an HTML event handler or an external image URL; the preview executes script or leaks its contents through a resource fetch. A model transcript emits malicious Markdown links or HTML. If such content reaches the privileged product webview, UI injection can reach whichever Tauri commands that webview owns. A Deno network denial does not constrain browser resource loads. Tauri scopes IPC access by webview/origin capabilities; it does not infer that a rendered string is untrusted. [Verified: Tauri capability reference](https://v2.tauri.app/reference/acl/capability/).

**Concrete fix.** Add a mandatory renderer policy to the ship blockers: text-escape filenames, field values, logs and source; disable raw HTML in Markdown; use a maintained sanitizer where markup is necessary. Prefer host-rendered invoice templates with escaped data. Any HTML preview needs an isolated surface with zero product IPC, no scripts, no external resources, no navigation/popups/downloads, and an explicit minimal CSP. Disable terminal-controlled clipboard writes and automatic URL opening. Test payloads in CSV cells, HTML drafts, manifest labels, transcript text, diffs, and terminal output. An HTML draft opened externally must also be safe as a document.

### 5. “Local-only” and deletion lack an operational barrier for active model jobs

**Severity: P0 for the promise; straightforward to specify before implementation.**

**Evidence.** PLAN §2.7 says local-only means no model egress. P5 (§8.3) only requires a local-only admission stop. §3.1 explicitly kills Muse hosts on lock/key removal, but does not specify the equivalent transition on local-only or deletion. A live Muse turn may make additional model/reminder requests after initial admission; §2.7 admits its wire bodies are unknown. §3.2 defines a capture/policy epoch but does not fully specify revocation enforcement for already admitted network operations.

**Failure inference.** The user switches local-only, sees a reassuring status, and the existing Muse host keeps sending. Or deletion removes local evidence while an admitted worker still holds a copy and sends it afterward. Stopping new jobs alone cannot satisfy either user expectation.

**Concrete fix.** Define a model-egress state machine and acknowledgment barrier. On local-only, stop admission, cancel queued jobs, revoke consent epochs, terminate/reap keyed Muse hosts, cancel direct transports, and reconcile unknown charges. Do not acknowledge local-only until the app can prevent further dispatch. State separately that already transmitted requests may complete and cannot be recalled. Distinguish Pause observation, Stop build/tools, Local-only, and Delete, including their effect on active work. Test switching/deleting during analyzer dispatch, a Muse tool step, reminder activity, and crash recovery. Apply deletion lineage to pending packets, test fixtures, outbound bodies, sessions, staging, undo, logs and their encryption keys; show any intentional retained tool separately.

### 6. The private MVP remains too large for a defensible solo schedule

**Severity: P1; highest delivery risk after the execution boundaries.**

**Evidence.** PLAN §1.3 includes six workflow families, native Show, contract editing, MSP recovery, a key-free terminal, a generated-code SDK/broker, independent validator, immutable signing, encrypted storage, selective deletion/key rotation, receipts, undo, rollback and repair. P1–P10 all block MVP. M1 gives storage/deletion, broker and validator two weeks; M2 gives six families plus the complete teaching/build/repair experience three weeks. M0 also asks for signed capture, API billing, two containment gates, generation, repair, clean installation/update and performance evidence in ten workdays. The source reports repeatedly label these implementations unverified; no app code exists in the brief.

**Assessment.** These are several substantial subsystems, not one vertical slice. AI coding agents do not remove permission testing, adversarial evaluation, signing, oracle authoring, or pilot support. The issue is not merely that 12–16 weeks might slip: the schedule front-loads infrastructure while the key product assumption—better than asking an existing agent for a converter—remains untested.

**Concrete fix.** Make the first private product **Tell → one CSV/text family → host-owned result comparison → save tool → manual rerun**. Keep create-only output, no network, no unattended work. Defer terminal, move/rename, invoice/report families, general undo, full repair UI, ambient mining and browser companion. Keep source/version integrity and permission enforcement. Defer capture storage by removing capture from this slice, rather than weakening capture security. Run the signed capture experiment separately. Schedule each platform/security uncertainty with a decision artifact before dependent UI. Re-estimate from measured build time and support effort; use the current dates as an optimistic capacity hypothesis only.

### 7. The pattern engine assumes action/dataflow signals the selected observer does not provide

**Severity: P1; blocks credible discovery claims, not Tell.**

**Evidence.** PLAN §6.1 expects safe control activation, file-selected/download-ready metadata, and semantic actions. §6.2 mines sequences of 3–12 actions and requires complete essential-step coverage. However, §5 specifies focus/state AX reads, no keyboard/clipboard observation, browser exclusion, and no concrete macOS click-event source. A folder watcher is excluded from MVP (§1.3). No adapter identifies how “file selected,” “export completed,” or “column mapping performed” becomes a reliable observed event. The older [macOS research](../research/06-capture-macos.md) had a CGEventTap source; the synthesis removes keyboard monitoring without specifying a replacement mouse/activation path.

AX notifications describe supported changes and can be unavailable. An AX press action is an operation a client invokes, not a universal log of user clicks. [Verified: Apple AX notifications](https://developer.apple.com/documentation/applicationservices/axnotificationconstants_h), [notification registration](https://developer.apple.com/documentation/applicationservices/1462089-axobserveraddnotification), [press action](https://developer.apple.com/documentation/applicationservices/kaxpressaction). UIA's event model also cannot be treated as evidence for macOS behavior. Masking editable contents further removes the very values needed to infer a CSV business rule, which the plan rightly requires the user to supply separately.

**Concrete fix.** Write an event-provenance matrix: every emitted verb, exact native/extension source, apps tested, permission, coverage and ambiguity. Unsupported verbs stay absent. Before committing to mining, manually demonstrate two target tasks with only the allowed signals and inspect whether the resulting sequence distinguishes them from negative controls. For metadata-only discovery, lower the claim to “similar app sequence; teach it,” not a buildable transformation. Add mouse observation only as a deliberately scoped, independently tested feature if justified. If no useful sequence survives, retain Teach/Tell and postpone discovery; do not broaden collection to rescue the precision target.

### 8. The first-user journey does not yet establish useful value in 30 minutes, and Builder is still developer-first

**Severity: P1; adoption/positioning risk.**

**Evidence.** PLAN's sample is honestly prebuilt (§2.2), so it demonstrates mechanics rather than solving the user's own task. The actual journey may require a separate Muse installation (large binary, version/signature/fingerprint checks), API account/key/billing/model availability, representative files, expected outputs, contract editing, cloud review, generation and staged approval. The onboarding wireframe has no missing-Muse/install/download/update failure path. §1.4 starts the generation clock after contract acceptance, excluding much of the user's work. The Builder wireframe gives the Muse transcript a full pane, plus Files, Tests, diff counts, context percentage and a terminal. Top-level Discoveries precedes Tools even though discovery is absent from private MVP.

**Assessment.** The narrow CSV result is sharp. The overall private-MVP interface is not. A technical BYOK user could plausibly finish in 30 minutes, but there is no demonstrated path from a fresh Mac/account. A non-developer is being asked to operate a small IDE and interpret test claims. “Task Contract,” “capability ceiling,” and “signed version” are useful engineering concepts, poor default user vocabulary.

**Concrete fix.** Separate “sample completed” from “my first useful tool.” Start the product clock at install/first launch, record dependency/key setup separately, and count abandonments. Lead with Tools and Describe; hide inactive Discoveries and advanced Build details. Default Builder to **Result → Rules → Examples checked → Access → Save tool**; make transcript/source/terminal optional details. Ask unresolved business questions using concrete examples, such as whether `03/04/26` means March 4 or April 3. Test the proposed journey and comprehension tasks below with fresh users before adding workflow families. A direct Meta artifact path can reduce the separate CLI installation burden if approved as the product path, while keeping Meta API billing.

### 9. The monthly cost arithmetic is correct, but the evaluation budget and repair limits contradict the release gates

**Severity: P1; blocks the stated evaluation, not a tiny smoke test.**

**Evidence.** Current [Meta rates](https://dev.meta.ai/docs/pricing-rate-limits) match §9.1: Spark $1.25 input/$4.25 output and Wonder $4/$20 per million, with the listed cache prices. At those rates the contract is $0.016, build $1.20 and repair $0.48. Eight builds/contracts plus two repairs = $10.688; sparse discovery adds $1.0175; total $11.7055, reserve $14.631875. The rounded plan values are correct. No cache discount is needed to obtain them.

But §9.3 requires 30 tasks × three fresh generations, against $50 per evaluation run. At the plan's baseline that is **$109.44**, including contract interpretation per attempt, before repairs, retries or reminders. Even generating the contract only once per task still costs $108.48. Forty repair cases at $0.48 add $19.20 if included, and healthy controls/other evaluations add more. A $50 cap can stop the release suite before its denominators exist. Separately, §9.1's $0.75 repair ceiling conflicts with §9.3's p90 allowance of $1.50 unless the evaluation and production admission policies differ explicitly.

**Concrete fix.** Split inexpensive deterministic regressions from fresh paid generation/repair evaluations. Budget a complete fresh baseline at roughly $130+ for the specified build and repair workloads before reserve; quote larger totals when actual retries/child calls require them. Chunking across runs is valid, but show the total evidence budget and identical frozen policy. Either align repair admission with the evaluated ceiling or evaluate at $0.75 and count exhausted attempts as failures. Replace token assumptions with measured per-attempt distributions, including automatic reviewer/reminder usage. Keep CLI/tool fees and signing/Windows test infrastructure separate from user API totals. Do not advertise the $11.71 workload estimate as observed typical spend or the local budget as a hard provider cap.

### 10. Windows is designed at API level, but its selected packaging/runtime path has no demonstrated delivery chain

**Severity: P1 for Windows commitment; not a reason to block the Mac Tell spike.**

**Evidence.** PLAN §5.4 correctly chooses WGC, normal-user UIA and packaged Windows OCR, and admits no Windows runtime testing. Windows.Media.Ocr's package-identity requirement is real. [Verified: Microsoft OCR namespace](https://learn.microsoft.com/en-us/uwp/api/windows.media.ocr). But Tauri's documented Windows installer routes are WiX MSI and NSIS, so selecting MSIX adds a packaging pipeline rather than a standard bundle switch. [Verified: Tauri Windows installer](https://v2.tauri.app/distribute/windows-installer/). Full-trust MSIX identity is also distinct from placing the Deno worker in AppContainer; the latter requires explicit token/capability/profile/ACL setup. [Verified: Microsoft packaged desktop behavior](https://learn.microsoft.com/en-us/windows/msix/desktop/desktop-to-uwp-behind-the-scenes), [AppContainer launch](https://learn.microsoft.com/en-us/windows/win32/secauthz/implementing-an-appcontainer).

Muse officially supports Windows and an installer; the old “no Windows Muse” research assumption is stale. Its docs also warn of possible initial sandbox UAC, which needs reconciliation with the plan's elevation kill criterion: one-time setup and an elevated observer are different behaviors. The docs do not prove this app's Windows MSP, private-home isolation or containment. [Verified: Muse Code platform support](https://dev.meta.ai/docs/muse-code).

**Concrete fix.** Keep early Windows compile/schema checks, but add a small packaged vertical spike before committing to M6/M7 dates: Tauri executable → MSIX identity → OCR synthetic image → selected WGC window → bounded UIA → external Muse start/auth → separately confined Deno → broker pipe → update. Record actual manifest, package activation and trust levels. Keep the observer medium-integrity and the compute worker confined. Decide whether documented first-time sandbox setup is acceptable; otherwise use the zero-tool route. Fund interactive x64 infrastructure and a physical tester before marketing Windows. On Mac, similarly prove picker-only versus broad Screen Recording access, helper attribution and signed-update behavior on each claimed OS; the picker is a distinct user-selection API, not an automatic grant for future windows. [Verified: Apple picker](https://developer.apple.com/documentation/screencapturekit/sccontentsharingpicker).

## What actually blocks the first two-week spike

The spike is authorized planning work, not permission to use stored consumer credentials, spend without the owner's experiment budget/key, or alter personal Muse login. The following are dependencies to settle, not reasons to stop useful synthetic/local work.

| Blocker | Required evidence / action | Work that can continue without it |
|---|---|---|
| Secure real-key supply and a working billing/model account | Dedicated developer key via secure native entry or a corrected private-input harness; fixed synthetic direct Spark call and API dashboard attribution | Echo transport, local UI/SDK, synthetic runtime tests |
| Latest MSP startup failure | Reproduce/classify `permission_profile_reviewer_unavailable`; explicit supported available posture; no fallback to account login | Direct Meta transport and zero-tool generation experiment |
| Unsafe probe defaults | Remove secret-bearing `--env` use, automatic approval, raw-secret logging and unmatched terminal acceptance from the real-key path | Inspect existing transcripts; use bogus-key canaries |
| Unproved keyed Muse confinement | Native file/web/plugin/child attack matrix under the exact private-home, shell-off launch; use fake credentials until enforced | Direct tool-free source generation; SDK design |
| Unproved runtime confinement | Signed chosen macOS helper/profile; denied reads/imports/network/processes, bounded caches/memory and broker-only effects | Trusted hand-written converter and preview flow |
| Persistent signing identity and toolchain | Check actual Rust/Tauri/Swift availability; build signed shell + helper; fresh-account picker/AX tests and synthetic canaries | Core contracts and unsigned developer-only experiments |
| Representative task and independent oracle | One fabricated orders CSV, explicit date/decimal rules, independently authored expected output and negative cases | Synthetic generic fixtures; no real-work capture required |
| Reviewable scope | Freeze one create-only CSV/text path; define staged-byte commit before integrating live writes | Separate capture feasibility experiment |

Do **not** run the historical `account` scenario: probe 13 already mutated the global Keychain item despite XDG isolation. The owner's keeper/login health is an independent operational check; Macroscope must not repair it through account/auth APIs. Also, the `goodkey` filename is not evidence of either a supplied API key or successful inference.

**A feasible spike deliverable:** one successful API-billed direct call; one correctly attributed MSP turn or a precise failure/fallback decision; one malicious generated-worker denial suite; one hand-checked CSV converter staged and committed by the broker; and a separate signed selected-window capture grant/revoke/pause experiment. Each result needs its exact command/profile/version, synthetic fixture, output and passed/failed/unknown status. Full updater, general repair, six workflow families, 3,000-placement release evaluation and retention claims do not belong in the two-week exit criterion. No real-key access means the paid gate stays open; it does not prevent the rest of this evidence work.

## UX judgment and a testable 30-minute journey

**Is v1 sharp?** The private core is: “Describe a repetitive file cleanup; keep a converter.” The public-v1 bundle—mining, browser observation and folder triggers—is a second substantial product increment. Present it that way. A five-destination app led by Discoveries makes an unfinished feature the front door. Begin at Tools with an obvious Create tool action and a small recent-results area.

**Does the first user get value in 30 minutes?** The sample can provide confidence in minutes; it is not personal value. A useful result means a tool for the user's own supported task that runs successfully on a second input. The current plan establishes no timed fresh-install evidence. Test two cohorts separately: users with a working Meta key/Muse installation and users starting without them. Do not remove setup failures from the denominator or time only the generation phase.

Proposed test sequence and timing targets, not measured promises:

| Time | User outcome | Required product behavior |
|---|---|---|
| 0–3 min | Understand one concrete use; try a clearly labeled bundled sample | No capture permission or model charge |
| 3–8 min | Choose their CSV and describe the desired output | Explain local file use; preview rows locally; stop clearly on unsupported requests |
| 8–13 min | Confirm columns, date meaning, decimal/rounding, missing-value behavior | Before/after examples and concrete questions, not schema editing |
| 13–18 min | Complete key/dependency checks and review the outbound packet | Guided missing-Muse path if MSP is required; disclose possible setup overrun, purpose, Standard mode, selected data, and estimated spend |
| 18–25 min | Receive staged output and meaningful test results | Show result first, generation progress honestly, and NEEDS YOUR CHECK alongside automated checks |
| 25–30 min | Save a named tool and run a second input | Commit exactly previewed bytes; open output; show selected-file scope and a receipt |

If fresh account/billing setup cannot fit, state the prerequisite before the download and measure it; do not redefine a prebuilt demo as a successful custom build. Compare the full workflow with describing the same CSV task to an existing coding agent. The product must reduce total explanation/review/maintenance effort, not just replace a terminal with a chat transcript.

**Trust and permission moments:** the separation of capture, analyzer egress, build authority and runtime grants is correct and must stay. Add four missing details. A Show demonstration operates on the real desktop, so advise a disposable sample and state that the user's manual actions can have real effects. The cloud sheet must distinguish structural/synthetic context from actual user data and say that schema, filenames, labels and business rules can still be confidential. Local-only needs the active-job barrier in finding 5. “Three clean runs” can reduce prompting only for the same enforced scope; it is a convenience decision, not evidence the program is safe on every future input. Show cancellation charges and provider-retention limits without making users read technical details to find them.

**Is Builder understandable to a non-developer?** Not as drawn. Replace the default transcript pane with: “Reads this file; produces this result; follows these rules; checked these examples; still needs your confirmation.” Make the primary action “Compare result,” then “Save tool.” Show an editable task summary, not “Contract v3”; show “Changes need checking again,” not artifact-signature language. Files/diffs/context/token details and terminal can remain an advanced drawer. An approval should say “Create one CSV in Reports; leave the source unchanged,” not “Approve permission ceiling.” Read-only HTML preview needs the security policy in finding 4 regardless of how friendly the UI is.

Before beta, ask five unfamiliar target users to complete one task without coaching, explain what data went to Meta, identify which file may change, stop an active build, deny Show permissions and continue with Tell, and recover from a bad column. Pass criteria: at least four complete their second-input run within 30 minutes with prerequisites available; all correctly identify read/write/cloud scope; no accidental grant or execution. This is a proposed formative usability gate, not a population success claim. Include at least two non-developers to test comprehension, while keeping the launch claim explicitly technical/BYOK until broader setup is proven.

## What the plan gets right and must keep

1. **Deterministic retained tools, rather than paid model replay on every run.** This makes reliability, cost and value inspectable. Keep runtime LLMs, computer control and arbitrary scripts outside the first release.
2. **Show/Tell before ambient discovery.** A cold-start user should not wait weeks for a suggestion or grant desktop observation merely to build a converter. Keep the honestly labeled local sample and denial paths.
3. **Business-rule confirmation and independent oracles.** Screenshots cannot reveal unstated date, reconciliation or invoice rules. Keep local representative input/expected output and host-owned tests; agent-authored checks cannot certify the agent's own work.
4. **App-owned key; API billing; no consumer-login workaround.** Direct API origin and Bearer auth are verified, and Muse's environment-key precedence is documented. Rejected-key probes really do close environment ingestion uncertainty, while proving neither paid success nor usage completeness. [Verified: Meta authentication](https://dev.meta.ai/docs/authentication), [Muse auth](https://dev.meta.ai/docs/muse-code/auth).
5. **Standard models and modest context.** Spark/Wonder and their API model IDs are documented; Wonder Full is subscription-only. The plan correctly avoids speculative SDKs, huge screenshot contexts and local-model policy exceptions. The public Muse SDK now also warrants an effort-reduction assessment, not an assumption that Rust must hand-maintain every protocol surface. [Verified: Meta models](https://dev.meta.ai/docs/models), [Muse SDK entry point](https://dev.meta.ai/docs/muse-code).
6. **Shell-off, no shared credential-store mutations, private HOME, and a tool-free fallback.** These directly respond to observed research incidents. Preserve them when integration becomes inconvenient.
7. **Broker-owned effects, immutable versions, explicit repair and uncertainty after crashes.** Keep changed-code reapproval, old-version preservation, honest undo limitations and no automatic replay of uncertain writes. Strengthen the staged-byte binding rather than removing review.
8. **Permission barriers, visible recording, conservative browser exclusion and missing-evidence labels.** Keep typed text/clipboard polling/audio out. Do not convert silence or a missing frame into evidence of a user action.
9. **Honest privacy/accounting claims.** Exact direct payloads versus supplied-to-Muse inputs is a useful distinction. Non-training is not universal zero retention; local deletion cannot recall provider data. The terms also distinguish incorporating outputs from downloadable materials, supporting the decision not to bundle Muse without clearance. [Verified: Meta terms §§1, 4–5](https://dev.meta.ai/legal/terms-of-service).
10. **Real denominators and platform evidence.** Retention must mature, failed accepted requests count, synthetic success cannot substitute for natural utility, hosted CI is not physical capture/battery evidence, and Windows needs an independent tester. Keep these principles while funding and right-sizing the gates.

## Open questions for the owner

- Can the spike use a dedicated, API-billed Meta key through a secure local entry path, and is the stated $10 generation experiment allowance actually available?
- Which one recurring CSV/text task and expected output should define the first personal-value test?
- Is a one-family Tell-first private product acceptable, with capture and MSP integration treated as separate evidence tracks until their gates pass?
- If safe MSP integration fails, is direct Meta zero-tool generation acceptable for the first release, with interactive Muse deferred?
- Who provides the persistent signing identity and later interactive/physical Windows testing budget? These are external prerequisites, not implementation defaults that code can assume.

## Decisions I made

- Ranked risks by blocked evidence, unauthorized authority and first-user value, not by how many components the architecture contains.
- Treated prior probes as historical evidence; did not rerun credential-mutating or paid scenarios.
- Accepted the verified Meta API/model/pricing facts; flagged live integration gaps and actual budget contradictions instead of repeating superseded doubts.
- Recommended one create-only CSV/text family and exact staged-output commitment as the first product slice.
- Kept the master plan unchanged; all proposed cuts and fixes are review recommendations.
