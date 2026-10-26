# Macroscope, summoned: teach a task, inspect its story, keep a tool

Contrarian UI/UX proposal, 22 October 2026.

Evidence convention: product behavior below is a **[assumption: proposed design]**, not a claim about existing implementation. Facts from checked repository material are labeled individually. User-testing predictions are hypotheses. No competitor behavior or unverified Meta product capability is required by this proposal. The shared brief says no app code exists yet. **[local: `plan/BRIEF.md`, read in full]**

## 1. The decision: an instrument drawer, not an operations center

Make Macroscope a small window you summon when you want help, with three verbs: **Run, Teach, Review**. Its enduring objects are named tools, such as “Prepare Tuesday status CSV,” rather than discoveries, conversations, or agents. Discovery works quietly in the background after explicit consent. Building happens in a temporary, resizable storyboard. A completed tool returns to the drawer.

The primary manufacturing gesture is **Teach this task**: demonstrate once, correct what Macroscope thinks happened, then ask it to build. The primary consumption gesture is selecting a tool and pressing its one large Run button. Passive discoveries arrive in a weekly review sheet, not an inbox demanding daily maintenance. The launcher is palette-shaped, but every core action also has visible buttons and pointer navigation. Keyboard expertise is optional.

This is meaningfully different from shrinking the standard window. There are no permanent Discoveries, Automations, and Builder destinations. A conversation is not the source of truth. The source of truth is a short, editable account of inputs, steps, expected output, permissions, and exceptions. Users can leave and resume it without reconstructing a chat.

**Product hypothesis:** users will better understand and trust one bounded tool than an assistant with an expanding memory of their desktop. That is unproven. The accepted trade-off is weaker ambient discovery and less fleet-management density, in exchange for an approachable first successful automation.

Do not make a global shortcut mandatory. Provide a macOS menu bar entry and Windows tray entry with Open, Teach, Pause observation, and Settings; allow a configurable shortcut and a normal application launch. These are proposed platform entry points, not a claim that their implementation has been tested. Keep the same vocabulary and state machine on both systems.

## 2. Observation is a choice, not the price of admission

First launch offers two equal routes:

- **Teach a task now.** Record a bounded demonstration after showing the capture scope. No ambient observation is enabled.
- **Find repeated work.** Explain exactly which apps and data types will be observed, obtain the required operating-system permissions, and start with an explicit app allowlist. Screen access and interaction-event access have separate explanations.

Neither route needs a Meta key until the first model request. Someone can explore, record, and edit a local draft before paying for generation. Explain at capture time that the demonstrated task runs against the user's real desktop: a recording is not a rehearsal or sandbox. Recommend a sample file or test account before recording a destructive workflow.

The teaching control stays visible while capture is active: app names, elapsed time, Pause, Stop, and Discard. Pause leaves an explicit gap in the storyboard. Resuming never silently fills that gap with inferred steps. Show a countdown before capture; never hide the recording indicator to obtain cleaner screenshots.

For ambient observation, the proposed default is app/window-transition metadata and event counts, with no typed text. Screenshot sampling is a separate opt-in. This restricts semantic discovery: detecting a repeated app sequence does not justify claiming that the user copied invoice amounts. If the product cannot find valuable patterns under those constraints, that is a product risk to measure, not permission to invent evidence.

Proposed retention: raw screen samples expire after 24 hours; local, coarse recurrence summaries expire after 14 days. A weekly digest may therefore contain counts without surviving screenshots. Label this honestly: “Similar app sequence seen 4 times; original screenshots expired.” Ask for a new demonstration when necessary. Do not retain secret screenshot thumbnails merely to make the digest persuasive. Manual teaching drafts can preserve only user-approved frames until the draft is deleted or its chosen expiry arrives.

No ambient screen contents reach Meta merely because observation is enabled. Opening a candidate prepares a local evidence summary; the user reviews the proposed outbound packet before a model call. Masking can fail, so the interface must show the actual redacted representation, not just a reassuring “private” badge.

## 3. Screen one: the summonable tool drawer

This is the daily screen, approximately a compact portrait window with enough room for readable titles. Search matches names and user-written descriptions. Empty search shows pinned tools, not algorithmic recommendations. Selecting a result exposes its inputs, effect, and exact current grant before Run.

```text
+-----------------------------------------------------------+
| Macroscope                       Observation: Paused [v]   |
| [ Search tools or describe a task...                    ] |
|                                                           |
| [Run]        [Teach this task]        [Review: 2 ideas]     |
|                                                           |
| PINNED                                                    |
| > Prepare Tuesday status CSV                         Enter |
|   Resize product photos                                   |
|   Create expense draft                                    |
|                                                           |
| PREPARE TUESDAY STATUS CSV                                 |
| Input  [Choose export.csv...]                              |
| Output A new CSV; the source stays untouched               |
|                                                           |
| [Read: chosen file] [Write: chosen output folder]           |
| [Network: none]                    [Inspect permissions]   |
|                                                           |
|              [       RUN WITH THESE INPUTS       ]         |
| Last run: completed, 18 rows       [Result] [Edit tool]     |
|                                                           |
| [All tools]     [Activity & data]               [Settings]  |
+-----------------------------------------------------------+
```

Selecting a tool never executes it. Enter runs only when the explicit Run control has focus, or after an equally explicit reviewed keyboard action. A result moving under the user's pointer cannot become an accidental execution. Avoid automatic clipboard inspection; “Use clipboard” is a deliberate input action with a preview.

Long-running work moves to a compact progress card that can be dismissed without cancelling the job. Closing the launcher means “hide”; a clearly labeled Stop button requests cancellation. Reopening restores progress. If the app cannot safely interrupt an external operation, say “Stopping after the current step” and preserve the eventual receipt.

The drawer also offers a browseable All tools list with text size controls, sort, archive, and search. This concedes a standard window's advantage when someone has many tools. Do not force 50 tools into a command palette forever.

Permission chips use concrete nouns: “Read: selected CSV,” not “Filesystem access.” Never truncate the scope to a misleading summary. A chip opens details, expiry, revocation, and the distinction between build-time access and runtime access. The prominent label is “Ask before creating a calendar event,” not “trusted.”

## 4. Discovery is an appointment, not a notification stream

Offer a weekly review time, with an in-app count by default and an optional system notification. Never show a candidate over another app. The review sheet contains at most three suggestions; the rest can wait. If the evidence is weak, show none. An empty week is acceptable.

Each suggestion states the observed recurrence, uncertainty, estimated benefit with assumptions, and proposed boundary:

> “A similar Jira-to-spreadsheet sequence appeared on 4 workdays. This might be the same task. If each occurrence takes about 6 minutes, a tool could save part of that time. Suggested first version: prepare the CSV; leave sending to you.”

Those numbers are illustrative, not product measurements. The copy must distinguish active interaction time from elapsed window-open time. Avoid an inflated annual savings counter.

Actions are **Show the evidence**, **Teach the real version**, **Not this task**, and **Never suggest from this app**. “Not this task” can suppress that pattern without making the user classify why. Opening a candidate does not authorize code generation or outbound data transfer. If the evidence is insufficient, the main action becomes “Show me once,” not “Automate this.”

Reject ambient hints such as “You've done this four times; want me to take over?” for v1. They expose observation at precisely the moment the user is concentrating, may reveal sensitive work during screen sharing, and frame a probabilistic inference as intimate knowledge. Those are design risks, not established behavioral findings. A persistent observation indicator provides awareness without narrating the user's activity.

Digest delivery has a real weakness: someone may forget the task before review. Preserve an editable local summary with dates and app names, respect expired evidence, and let users save an idea immediately via the launcher. Do not solve forgetting by retaining everything.

## 5. Screen two: correct the storyboard before writing code

The storyboard is a temporary workspace, not a chat history. It expands beyond the launcher and can become a full window when the sequence is complex. The user checks a small number of semantic steps, each linked to evidence where available. Observation, inference, and user instruction have different text labels; color alone does not carry that distinction.

```text
+---------------------------------------------------------------------+
| < Tools       Prepare Tuesday status CSV          Draft saved locally |
| 1 Describe  >  2 Check story  >  3 Build  >  4 Prove                  |
|                                                                     |
| RESULT: [A CSV of open items, dates YYYY-MM-DD, owner included      ] |
| INPUT:  [A CSV exported by me]       [Use a sample file]              |
|                                                                     |
| STORY                                      SELECTED STEP            |
| 1 [You said] Choose the Jira export         3. Normalize dates        |
| 2 [Observed] Read six columns               [Approved frame preview] |
| 3 [Inferred ?] Normalize dates        --->   I inferred this rule.    |
| 4 [You said] Save a new output CSV          Date source: [created v]  |
| 5 [Excluded] Send an email                  Format: [YYYY-MM-DD v]   |
|                                            If blank: [Ask me v]     |
| [+ Add step] [Reorder] [Exclude]            [Correct this step]      |
|                                                                     |
| CHECK BEFORE BUILD                                                  |
| ? Should closed items be excluded?          [Yes] [No] [Ask each run] |
| ? Two exports open: which is the input?      [Chosen file only]       |
|                                                                     |
| [What will leave this computer?]      [Build reviewed story]          |
+---------------------------------------------------------------------+
```

Offer plain-language corrections (“Use the invoice date, not today's date”) beside structured fields. Corrections update visible steps; the user accepts the resulting diff. Preserve the prior version. Avoid a second hidden conversation that can override the storyboard without changing it.

The critical feature is **separating intention from incidental behavior**. Opening Slack during a demonstration should be removable. The destination filename might be an input, while the desired date format is a rule. “Repeat these clicks” is a different tool type from “produce this result”; explain which is being built. Favor output-oriented tools in v1 because their success can be inspected using files and records. A visual replay that depends on screen coordinates should visibly disclose that dependency.

Before build, resolve material unknowns: input selection, output location, overwrite policy, external destinations, and irreversible actions. Noncritical ambiguities may become an explicit “Ask each run” input. Do not block on cosmetic naming. A failed inference should be cheap to correct, not defended by the assistant.

The outbound-data sheet lists text fields and thumbnails in the exact prepared request, including any selected code or error excerpts. Users can remove a frame, mask a region, or replace a value with a placeholder. Show separate consent for future repair requests; past approval does not cover newly captured evidence. Describe downstream retention as unknown unless verified, and explain that deleting local data does not establish deletion at the provider.

## 6. Build without making the transcript the product

During generation, display milestones tied to artifacts: “Creating the converter,” “Checking it on sample rows,” and “Ready to compare.” Only report checks the host actually ran. If all the app knows is that a model call is active, say “Generating” rather than inventing progress or a completion percentage.

The brief records an installed Muse CLI, headless JSONL operation, and API-key authentication; the checked help includes `muse exec --json`, `--api-key-stdin`, `--output-schema`, and workspace/safety controls. **[local: `plan/BRIEF.md`; `plan/reference/muse-cli-help.txt`]** Use a supervised headless build for v1. A persistent MSP conversation and embedded terminal are unnecessary for this interaction model.

The precise proposed key path is: user pastes the owner's Meta developer API key into a secure settings field; the native app stores it in the platform credential store; immediately before generation, a credential broker retrieves it and supplies it through a private stdin pipe to a new `muse exec --provider meta --api-key-stdin --json` child. The prompt comes from a separate protected request file. Never put the key in argv, the storyboard, prompt, transcript, generated tool, analytics, or shell history. Credential-store integration and successful real-key operation remain implementation acceptance tests. The brief's successful test established rejection of a bogus key, not a successful billed request. **[local: `plan/BRIEF.md`]**

Keep model/provider configuration configurable; do not depend on an unverified endpoint or model capability. The brief reports `muse-wonder-1.0` used by a local test, but that is not a promise of future availability. **[local: `plan/BRIEF.md`]** No consumer login is part of onboarding. Do not depend on the unverified `muse serve` environment-auth path.

For narrow v1 converters, request a structured artifact and have the host stage it, instead of giving the coding process unrestricted desktop access. Disable shell and workspace writes where compatible with that build path; admit only required tools. CLI flags alone are not proof of complete isolation. The engineering acceptance gate is that the builder cannot read arbitrary user files or invoke unapproved external tools, and generated code runs in a separate constrained process without the Meta key. If that cannot be demonstrated, restrict the tool type or block generation rather than displaying an unenforceable permission claim.

“Build details” exposes sanitized events, generated files, and validation results for technical users. It does not expose hidden reasoning or promise a complete rationale. An embedded terminal would create another permission surface and is deferred. This deliberately sacrifices some debugging power.

Before a paid call, show the data packet and user-set spending limit. Dollar estimates require verified pricing and are otherwise omitted. Reserve budget before starting concurrent calls; reconcile actual usage afterward. If a strict provider-side dollar cap is unavailable, explain the limit's precision rather than promising exact enforcement. Cancelling stops further work where possible; it does not promise to erase charges for an in-flight request.

## 7. Screen three: the proof bench and run contract

“It compiled” is not the trust event. The user needs to see that a representative input produces the intended result, with the promised boundary. The proof bench combines a sample comparison with the proposed run contract.

```text
+---------------------------------------------------------------------+
| < Story                Prove this tool                     Version 1 |
|                                                                     |
| SAMPLE INPUT                         ACTUAL OUTPUT                   |
| 18 rows from sample-export.csv       18 rows in status-preview.csv   |
| [Inspect input]                      [Open output] [Compare fields] |
|                                                                     |
| CHECKS                                                              |
| [PASS] Dates match the agreed format                                 |
| [PASS] Missing owner becomes "Unassigned"                             |
| [PASS] Original file unchanged                                      |
| [NOT TESTED] A 50,000-row export                                     |
| [NEEDS YOU] Does this include the right tickets?                     |
|                                                                     |
| WHEN YOU RUN                                                        |
| Reads: one CSV you select          Writes: one new CSV                |
| Network: none                      Background schedule: off          |
| If output exists: ask              Sending or deleting: unavailable  |
|                                                                     |
| [Correct the story] [Try another sample]                             |
|                                                                     |
| [Keep as draft]                    [Save tool: ask for a file each run]|
+---------------------------------------------------------------------+
```

Tests must include meaningful examples: a normal case, missing fields, invalid dates, and an existing destination. Some expected values come from the user's confirmed story; others need manual judgment. A model can suggest checks but cannot certify its own output as correct. Use Passed, Failed, and Not tested labels with concrete scope, never a generic “Safe” score.

For file transformations, a test uses staged copies and a new output location. For actions involving real services, distinguish preview, genuine service sandbox, and live limited trial. If no sandbox exists, say so. Default to stopping at a prepared draft before sending, submitting, deleting, or paying. An “Undo” control appears only when a compensating action really exists; otherwise offer recovery instructions and an activity receipt.

Saving binds a tool version to an explicit grant and input policy. Passing one test does not authorize scheduling or sending. The initial v1 grant is foreground, user-triggered, with selected inputs. Later “Use this folder every run” and schedules require a scope review. A scope change invalidates the relevant grant; a code change requires regression checks and visible version review even if scopes remain identical.

## 8. The complete loop, including failure

```text
Weekly review OR Teach now
        |
        v
Local evidence -> correct story -> review outbound packet + cost
        |                                  |
        | discard                          v
        +------------------------- supervised Muse build
                                           |
                                           v
                                staged test + compare output
                                  |                   |
                               correct             confirm result
                                  |                   |
                                  +-> story           v
                                              save version + grant
                                                      |
                                                      v
                                                 explicit Run
                                                /            \
                                         receipt          stop / failure
                                                              |
                                              prepare repair evidence
                                                              |
                                              review packet -> new build
                                                              |
                                               regression + scope review
                                                              |
                                              activate repaired version
```

**Discovery:** a user sees a recurring export sequence in Friday's review. The evidence cannot establish the intended filtering rule. They demonstrate with a sample export and exclude the incidental email step.

**Build:** they correct “updated date” to “created date,” choose a new-file output policy, inspect the outbound request, add their key, and approve generation. The app preserves the story if the key is rejected or the quota is exhausted. Retrying never silently increases the approved budget.

**Test:** a sample exposes a blank owner. They choose “Unassigned,” rebuild the affected version, and compare both normal and blank-owner cases. The app records which version and sample hashes produced the result, without retaining full input indefinitely.

**Trust:** they save the tool with “Ask for a CSV each run.” Next Tuesday they select a file and run. A receipt links input identity, tool version, output location, actual permissions exercised, and any unverified conditions. It does not expose raw sensitive values in the launcher.

**Repair:** a later export changes a column name. The tool stops before writing, explains “Expected created_date; found created,” and offers Choose another input, Correct mapping, or Prepare a repair. It never silently rewrites itself against live data. The repair packet previews the relevant schema and error; the user can redact it before another Meta call. After generation, test both the new sample and retained approved fixtures. Keep the previous version available, but never rerun a partially completed external action automatically. Unknown outcomes require reconciliation before retry.

Repair should feel like correcting an appliance's instructions, not starting a new agent conversation. The best repair is a visible input mapping update that needs no model call. That saves time, money, and another data disclosure.

## 9. Where this beats the standard model, and where it loses

| Situation | Better model | Reason and concession |
|---|---|---|
| Run one familiar tool | This proposal | Short path from summon to inputs to Run; a full window adds navigation. Must validate speed with users. |
| Correct a misunderstood workflow | This proposal | A visible storyboard localizes the mistaken rule; chat may bury it across turns. Complex branching will strain this editor. |
| Decide what a tool can do | This proposal | A contract sits beside the action and is bound to a version. A dedicated privacy center still offers better global inspection. |
| Notice useful work immediately | Standard | A quiet digest trades immediacy for fewer interruptions. Teaching also transfers discovery work back to the user. |
| Manage many tools and schedules | Standard | Persistent navigation and dense management views scale better. The drawer needs an eventual expanded library. |
| Diagnose arbitrary generated programs | Standard | Transcript and terminal provide developer power. This proposal deliberately narrows v1 tool types. |
| Establish global capture/data policy | Standard | A privacy destination is easier to audit comprehensively. This design must keep Activity & data one click away. |
| Serve occasional, nontechnical use | This proposal, hypothesis | Three concrete verbs and visible outputs may reduce learning. A hidden shortcut could reverse that advantage. |

The standard architecture can adopt story editing and contracts without becoming palette-first. Those two ideas are stronger than the claim that a small window is inherently better. My recommendation is therefore firm about the object model and cautious about window size: ship tools, stories, proofs, and grants; test whether the drawer actually improves access.

## 10. Accessibility, privacy operations, and launch gates

Every storyboard action needs a labeled list alternative: Move up/down, edit, exclude, and jump to evidence. No drag-only timelines. Preserve focus after generation and errors; announce status changes without reading every streaming token. Support keyboard traversal, screen readers, reduced motion, resizing, and large text. These are requirements to test on macOS and Windows, not claims of existing compliance. Do not ship an untested Windows global shortcut or permission flow as equivalent to the Mac experience; use automated checks and recruit Windows validation before declaring parity.

Activity & data is a compact, reachable control sheet: observation state, allowed apps, recent captures, expiry, outbound request history, running jobs, grants, and deletion. Pause observation does not stop a running tool; Stop tool does not revoke capture consent. State both consequences plainly. Deleting a tool offers separate deletion of its samples and build history. Revoking a grant prevents future uses and explains what can still finish in flight.

Product-killing risks and corresponding gates:

- **Capture permissions and deployment:** OS consent or packaging restrictions may make the proposed recorder unreliable. Gate teaching on actual permission checks and test the full signed/distributed build. App Store eligibility, notarization, and Windows distribution requirements are unresolved external constraints, not assumed approvals.
- **Privacy and policy:** screen masking may miss secrets; Meta API terms may limit the intended use or retention assumptions. Verify provider terms and real outbound payloads before enabling remote screenshot analysis. If verification fails, keep manual text stories and bounded file tools available, or stop that feature.
- **Prompt injection:** captured pages and documents are untrusted data. Their instructions cannot grant permissions, change destinations, or bypass the confirmed story. Enforce capabilities outside model output. A chip without enforcement would undermine the product's trust premise.
- **Cost:** ambient cloud analysis could consume budget before producing a useful tool. Start with local coarse discovery and paid calls at explicit boundaries. Measure total paid cost per retained useful tool, including unsuccessful builds and repairs.
- **Execution containment:** arbitrary scripts may escape the proposed boundary or produce partial external effects. Start with bounded converters and draft creation. Broader autonomous GUI control waits for enforceable isolation, recovery behavior, and user-visible evidence.
- **Value:** if explicit teaching is laborious and passive discovery remains vague, this becomes another automation builder. Test whether the confirmed story plus first proof takes less effort than the task repetitions it replaces.

Prototype validation should use six target users as a directional study, not statistical proof. Give each the same file task, a mistaken inference, a permission expansion, and a partial failure. Compare against the standard flow. Proposed success criteria: at least five can explain the tool's scope before running; at least five correct the mistaken rule without help; nobody mistakes preview for live execution. Measure time to useful tested output, accidental execution, abandoned drafts, recall of outbound data, and repair success. A fast launcher that worsens those comprehension measures loses.

## Open questions for the owner

- Are you willing to restrict v1 to file transformations and prepared drafts, delaying arbitrary desktop execution until its permissions and recovery can be enforced?
- Should ambient screenshot sampling be opt-in as proposed, accepting weaker discovery, or is discovering meaning from the screen essential to the initial value proposition?
- Is a quiet weekly review acceptable for the first target users, or do their workflows require user-configured daily review to remain recognizable?
- Who can validate Windows permissions, accessibility, and execution behavior before Windows is offered as supported?

## Decisions I made

- Use a summonable Run/Teach/Review drawer with a visible launcher entry and an expandable tool library.
- Make corrected stories, versioned tools, proof results, and enforced grants the persistent objects.
- Keep ambient observation optional and remote evidence transfer separately reviewable.
- Deliver discoveries in a quiet weekly review; omit ambient intervention overlays.
- Use supervised API-key-authenticated Muse exec builds; defer a persistent terminal and consumer login.
- Require staged proof before saving; require new review for changed behavior or scope.
- Treat repair as a new reviewed version and never silently retry uncertain external effects.
- Prefer the standard window when library management, schedules, or deep debugging become the dominant job.
