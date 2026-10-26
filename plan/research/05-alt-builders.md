# Building software inside Macroscope: alternatives and fallbacks

Research date: 2026-10-22. Scope: the builder, its generated artifacts, and their execution contract. Read against the complete [brief](../BRIEF.md), [CLI help](../reference/muse-cli-help.txt), and stable [MSP declarations](../reference/msp/msp.d.ts).

Evidence convention: **[verified: URL]** denotes public primary documentation; **[local: checked on machine]** denotes the supplied local evidence or an explicit inspection below; **[assumption]** denotes proposed design, estimates, or unresolved behavior. Design sections and examples are proposals throughout, not existing Macroscope functionality.

## Recommendation: own the automation contract, reuse the coding agent

**[assumption]** Ship two artifact tiers. The default is a small typed workflow graph, rendered as understandable steps and executed by Macroscope's capability broker. Meta generates or repairs that graph. The advanced tier is actual code built by Muse Code in an isolated project, preferably controlled through MSP with a native product UI. An embedded terminal is a useful advanced view and fallback, not the default experience.

**[assumption]** These are complementary layers rather than six competing products:

```text
Observed repetition -> proposed outcome -> user selects "Automate this"
                                             |
                                   Builder request + fixtures
                                  /                      \
                   Meta graph generator             Muse Code
                   constrained repair loop          MSP / exec / PTY
                                  \                      /
                           versioned artifact + evidence
                                        |
                    schema checks -> replay -> permission review
                                        |
                           Macroscope runtime / broker
                          /          |           \
                       files      browser     desktop UI
```

**[assumption]** A saved automation should normally run without an agent. Reformatting a CSV is deterministic work; paying a model to rediscover the steps on every run increases cost and variability. Use an LLM node only where interpretation is necessary. A custom form or menu-bar tool can be a schema-generated parameter screen over the same graph. Full code becomes necessary for new algorithms, unusual formats, richer interfaces, or integrations outside the catalog.

## Comparison and implementation choices

**[assumption]** Relative effort below is a solo-developer planning judgment, not a benchmark. Each option still needs the same independent artifact validation and permission review.

| Route | Best use | Product experience | Main cost / limitation | Decision |
|---|---|---|---|---|
| Muse MSP | Interactive full-code builds and repair | Native progress, diffs, structured approvals, resume | Stateful protocol and CLI lifecycle | Preferred full-code integration |
| Muse embedded TUI | Expert development and debugging | Familiar terminal, immediate feature parity | Terminal output is a poor product-state API | Advanced fallback |
| Muse exec JSONL | Bounded scaffold, migration, repair jobs | Job card, logs, resulting diff | Limited interactive control | Earliest full-code prototype |
| Own Meta loop | Graph generation and constrained repair | Precise steps and predictable controls | We own retries, history, validation | Default graph builder |
| Codex / Claude adapters | Explicit alternative provider choice | Same artifact review | Separate credentials, policy and compatibility | Optional later |
| Typed DSL runtime | Running routine automations | Inspectable steps, preview and undo | Catalog limits expressiveness | Default artifact target |

### 1. Muse Code through `muse serve`: native UI

Muse documents `muse serve` and the `@muse-code/sdk` TypeScript wrapper; MSP is its session protocol, not MCP. MCP is separately used to connect tools. **[verified: https://dev.meta.ai/docs/muse-code]** The local binary is Muse Code 1.4.11; the schema manifest reports version 1 and a stable fingerprint. **[local: checked on machine: `muse --version`, `plan/reference/msp/manifest.json`]**

**[assumption]** Run a child process owned by the desktop supervisor. Use pipes, not a PTY, for MSP. Prefer a small adapter using the official SDK if its packaged version matches the installed host; otherwise generate typed protocol bindings from the exported schema. For Flutter, keep protocol/state reduction in a Rust or TypeScript helper and send product events to Dart. Avoid implementing all 3,651 lines in the UI.

The useful minimum is concrete:

1. `initialize` identifies the client and returns schema identity, granted capabilities, platform and sandbox availability. Absent sandbox availability means unknown, not safe.
2. `session/start` sets workspace, model, provider and approval mode; `turn/start` submits typed input. Both use client-issued command IDs. An accepted request is admission, not task completion.
3. Fold `item/started`, `item/delta`, `item/completed` into a transcript and separate artifact diff. Treat unknown item kinds generically.
4. Render `approval/request` using the provided choices. Send `approval/decide` with both `commandId` and current `requirementId`; a stale approval must not authorize a newer stage.
5. Persist the opaque `viewCursor`; resume with `session/resume`. `session/read` is a snapshot read and does not attach or reissue approvals. Resume returns pending-request pointers and reissues requests.
6. Use `turn/completed.terminal`, typed errors, token usage and quota reset details to settle the run. `authRequired` and `quota.resetsAtMs` enable useful recovery UI.

All six are **[local: checked on machine: inspected `InitializeResult`, `SessionStartParams`, `TurnStartParams`, item events, approval types, `SessionResumeResult`, `TurnCompletedParams`, `TurnError` and `QuotaDetail` in the supplied MSP declarations]**.

**[assumption]** Keep separate hosts for separate sandbox postures. The local help says sandbox and durability are fixed at host construction; approval mode is selected on the wire. Do not attempt to loosen sandboxing using arbitrary `session/start.config` keys: its currently declared member is `mcpServers`, not a general policy document. **[local: checked on machine: CLI help and `SessionConfig`]**

**[assumption]** Initial UI: “Understanding task”, “Changing files”, “Testing”, then “Ready to try”. Show touched files and test evidence, with an expandable technical transcript. On reconnect, show “Recovering build” until replay settles. Unknown approval subjects default to denial. Never map wire `allowAll` to a friendly “safe” button. SDK capabilities such as `userShell` are protocol grants, not substitutes for OS isolation or the runtime's own permissions.

**[assumption]** MSP is the strongest long-term choice because it exposes actual agent state without interpreting a screen of terminal text. Its trade-off is engineering effort in replay, idempotency, process crashes and version compatibility. Pin tested versions and keep golden protocol transcripts; do not let a background CLI update silently change production behavior.

### 2. Muse TUI in an embedded PTY

`muse` provides the interactive terminal interface. **[verified: https://dev.meta.ai/docs/muse-code]** Three components must be distinguished:

| Component | Actual responsibility | Implication |
|---|---|---|
| xterm.js | Browser terminal emulator | Embed local assets in a WebView; it does not provide the child process |
| SwiftTerm | Swift terminal engine and Apple views, including a local-process view | Good native Mac option; requires a distinct Windows integration |
| portable-pty | Rust cross-platform PTY API with process, read/write and resize support | Useful process layer beneath a renderer; not a UI |

**[verified: https://xtermjs.org/; https://github.com/migueldeicaza/SwiftTerm; https://docs.rs/portable-pty/latest/portable_pty/]**

**[assumption]** For a cross-platform shell, choose xterm.js plus portable-pty, joined by a tightly scoped local IPC bridge. If the product chooses native Swift UI, use SwiftTerm on Mac and accept a second renderer on Windows. Test Windows PTY behavior in Windows CI and with an external tester; a Mac PTY test cannot establish Windows correctness. Muse documents Windows support, PowerShell-based commands and possible first-time sandbox UAC prompts. **[verified: https://dev.meta.ai/docs/muse-code]**

**[assumption]** Spawn the Muse executable directly with an argument array, controlled environment and project working directory. Handle resize, Unicode, bracketed paste, keyboard navigation, suspend/resume and terminating the entire process tree. A terminal is not a sandbox. Do not use terminal text parsing to decide that tests passed, an approval was granted, or a build may be installed. Inspect artifacts independently.

**[assumption]** Disable terminal-driven clipboard writes and unreviewed hyperlink opening; isolate its WebView from application secrets and privileged bridge methods. Terminal output is untrusted, consistent with xterm.js's security guidance. **[verified: https://xtermjs.org/docs/guides/security/]**

**[assumption]** This is the fastest way to satisfy “Muse inside the app” literally. It is weaker for users who want the result rather than an IDE: approvals, failures and navigation remain terminal concepts. Offer “Open builder terminal” on an advanced screen, preferably handing off a session after releasing the existing host's ownership rather than allowing two writers.

### 3. `muse exec --json`: bounded background jobs

The installed CLI supports JSONL, `--output-schema`, `--prompt-file`, `--workspace`, worktrees, `--max-model-steps`, `--session-id` and `--api-key-stdin`. **[local: checked on machine: supplied exec help]** Public documentation confirms headless execution and warns that exit zero means the turn completed, not that generated work is correct. **[verified: https://dev.meta.ai/docs/muse-code/extending]**

**[assumption]** Use an argument-array equivalent of:

```text
muse exec --provider meta --json
  --workspace <disposable-project>
  --prompt-file <app-owned-task.txt>
  --output-schema <app-owned-build-result.schema.json>
  --max-model-steps 12 --disable-request-user-input
  --api-key-stdin
```

**[assumption]** The result schema contains artifact paths, summary, claimed tests and unresolved limitations. The supervisor computes actual file hashes and runs its own checks; model claims are not evidence. Parse stdout incrementally with bounded buffers; capture stderr separately and redact it. Limit wall time, subprocess count and log size as well as model steps. Kill descendants on cancellation.

**[assumption]** Keep sandboxing on. A task requiring unavailable authority should fail into a “Needs access” state, then be continued interactively through MSP; do not silently retry with `--yolo` or treat a CLI approval-mode name as equivalent to an MSP enum. Establish headless denial behavior with the pinned build before shipping. `--output-schema` constrains the final message, not code edits or side effects.

**[assumption]** This route is ideal for the first vertical slice and batch repair. It remains useful after MSP exists. It should not become the scheduler for ordinary saved automations: building software and executing software are different operations.

### 4. Our own small loop calling Meta directly

Meta currently documents `https://api.meta.ai/v1`, bearer API-key authentication, `muse-spark-1.3` and `muse-wonder-1.0`; Wonder is preview. Earlier guessed endpoints are unnecessary. **[verified: https://dev.meta.ai/docs/overview]** Meta recommends its Responses format for new agent loops and supports an OpenAI-compatible client against the Meta endpoint. That is Meta inference and billing, not an OpenAI model dependency. **[verified: https://dev.meta.ai/docs/protocols]**

**[assumption]** Implement only four builder tools initially:

```text
read_file(projectHandle, relativePath)
write_file(projectHandle, relativePath, expectedHash, contents)
run_test(projectHandle, testProfileId, fixtureSetId)
read_ui_tree(previewHandle, rootId, maxNodes)
```

**[assumption]** `write_file` can edit only workflow manifests, schemas and fixtures in staging. `run_test` selects an application-owned validator/replayer; it accepts no shell string, executable path or model-defined package script. Do not execute a model-edited test harness under the label “constrained tools”. `read_ui_tree` reads the local preview or an explicitly authorized, filtered app subtree, never the entire desktop by default. No general shell, arbitrary imports, package installation or unrestricted HTTP tool.

Meta returns function-call items for the application to execute; results are returned with the matching `call_id`. Its documented `tool_choice` supports only `auto`, and `max_tool_calls` does not cap client-side function calls. **[verified: https://dev.meta.ai/docs/tool-calling]**

**[assumption]** Therefore enforce limits in our loop: eight model requests, twenty tool calls, two repair attempts, output-byte ceilings and a per-build token budget. Wait for complete arguments, validate types and authority, then execute serially. Save checkpoints and safe tool errors. Plain text without a valid artifact is “needs clarification”, not success. Preserve the selected API's reasoning replay contract and recheck current schemas before implementation.

**[assumption]** This is manageable for graph generation. It is a poor use of a solo developer's time to recreate a general coding harness, context compaction, dependency repair and shell-policy system. Delegate those jobs to Muse. Direct-loop failure can fall back to Muse generating the same graph; it must not automatically escalate to arbitrary code.

### 5. Optional agent backends and the Builder boundary

Codex app-server documents JSONL over stdio, initialization, thread/turn methods, streamed items, approvals and API-key login through `account/login/start`. Its schema generators emit bindings for the installed version. **[verified: https://learn.chatgpt.com/docs/app-server]** Local Codex is 0.163.0. **[local: checked on machine: `codex --version`]**

Claude Agent SDK exposes the Claude Code loop through TypeScript and Python, with sessions and tools. **[verified: https://code.claude.com/docs/en/agent-sdk/overview]** It supports permission rules and `canUseTool`; these controls still require our artifact and OS boundaries. **[verified: https://code.claude.com/docs/en/agent-sdk/permissions]** Its quickstart specifies `ANTHROPIC_API_KEY` and restricts third-party consumer-login offerings without prior approval. **[verified: https://code.claude.com/docs/en/agent-sdk/quickstart]**

**[assumption]** Keep both optional and disabled initially. Their normal integrations need separate provider keys and explicit user selection. Do not silently send captured activity to another company on a Meta failure. Meta documents compatible Responses and Messages formats, but this alone does not prove every Codex or Claude harness works correctly against Meta; treat such routing as a separate compatibility experiment. **[verified: https://dev.meta.ai/docs/protocols]**

**[assumption]** Define a small provider-agnostic boundary now, without building every adapter:

```ts
interface Builder {
  capabilities(): BuilderCapabilities;
  start(request: BuildRequest): AsyncIterable<BuildEvent>;
  resume(ref: BuildRef): AsyncIterable<BuildEvent>;
  answer(ref: BuildRef, answer: ApprovalOrInput): Promise<void>;
  cancel(ref: BuildRef): Promise<void>;
}
// Request: outcome, target="graph-v1"|"code", workspaceHandle,
// fixtures, permissionCeiling, model, credentialRef, budget.
// Events: phase, message, artifactChanged, approvalNeeded,
// usage, failed, completed. Preserve opaque backend references.
```

**[assumption]** Capabilities declare native approvals, resumability, steering, structured artifacts and terminal handoff. Unsupported features stay unavailable; do not emulate every backend as MSP. Keep provider-native events for diagnostics behind redaction. `completed` means builder completion; only the host's validator can mark an artifact installable. Provider choice must not own runtime permissions, stored automations or the user's library. This boundary buys resilience to CLI changes, outages and evaluation results without diluting the Meta default.

## API-key path: explicit for every Meta route

Muse's published precedence is `META_API_KEY`, stored key, then browser session. **[verified: https://dev.meta.ai/docs/muse-code/auth]** The brief's bogus-key experiment established exec's environment-key path, not a successful billed request or serve-specific inheritance. **[local: checked on machine: evidence recorded in `BRIEF.md`]**

**[assumption]** Settings accepts the owner's Meta developer key into an OS credential-store abstraction. The frontend receives a credential reference and masked suffix only. The native supervisor retrieves the key just in time:

| Route | Exact handoff |
|---|---|
| MSP | Put key in the `muse serve --provider meta` child's `META_API_KEY` environment; keep stdin exclusively for MSP |
| TUI | Same child-only environment for direct PTY launch of `muse --provider meta`; never type the key into the terminal |
| Exec | Pipe key through `--api-key-stdin`, with prompt in an app-owned file; alternatively use child-only `META_API_KEY` |
| Direct loop / DSL LLM node | Native HTTP client sends `Authorization: Bearer <key>` to approved Meta origin; no key in graph or WebView |

**[assumption]** Reject missing app credentials before spawning; never fall through to a consumer login or subscription billing. Scrub unrelated inherited provider credentials and redact protocol auth messages, crash reports and logs. Avoid `muse auth set` as the default because it creates a second durable key store outside our settings lifecycle.

**[assumption]** Important unresolved risk: a Muse process with a key in its environment may expose it to descendants or agent-accessible process state. The supplied evidence does not establish secret isolation. Test inheritance; use no-shell mode for graph builds and a brokered credential gateway or stronger process isolation before promising protected credentials in full-code builds. Passing stdin avoids argv exposure but does not prove isolation inside Muse. Key rotation restarts long-lived hosts. Acceptance requires real-key exec and serve calls, rejection tests with stored consumer sessions present, and verification that billing uses the owner's API project.

## The Macroscope SDK/DSL contract

### A graph with typed effects, not JavaScript disguised as data

Apple Shortcuts documents actions as composable steps with typed content flow. **[verified: https://support.apple.com/guide/shortcuts-mac/intro-to-shortcuts-apdf22b0444c/mac]** **[assumption]** Borrow that inspectability, but keep our portable format and runner. Export to Shortcuts later where mappings are exact; do not make a Mac-only format the canonical Windows artifact.

**[assumption]** Store canonical JSON with `sdkVersion`, artifact version/hash, parameter schema, triggers, requested capabilities, nodes, fixtures and expected results. Nodes have stable IDs, versioned opcodes, typed inputs and explicit output references. Expressions support references, comparisons, formatting and a small pure transform catalog. No `eval`, embedded scripts, arbitrary regex execution, recursive subflows or unbounded loops. A bounded `forEach` has an item limit; each node has timeout and output limits.

**[assumption]** Generate TypeScript authoring bindings for developers, but compile into this graph before installation. TypeScript is an authoring convenience, not a security boundary. New native primitives require app development and review. Advanced arbitrary code is visibly classified separately and executed in a sandboxed worker without unrestricted IPC or direct access to desktop privileges.

### Primitives and permissions

**[assumption]** The following is the proposed v1 surface. Capabilities are enforced by the broker on every invocation, never by generated code voluntarily calling a checker.

| Primitive / typed result | Capability and enforced scope |
|---|---|
| `clipboard.readText({maxBytes}) -> Text` | `clipboard.read`; manual gesture initially; exclude secure contexts; no history access |
| `files.list/read/writeAtomic/move/trash(root, relativePath)` | Separate read/write/delete root grants; opaque picked-root handles; canonical-path and symlink escape protection; preimage hashes and backups |
| `http.request(service, route, method, body) -> Json` | Exact origin/method/path templates, response limits, credential reference and declared data fields; validate redirects; block undeclared loopback/private destinations |
| `browser.open/readTable/fill/click/download` | Profile, allowed origins, read versus act; semantic locators and expected page state; downloads restricted to granted root |
| `ui.readTree/readValue/setValue/press` | App identity plus selector and action allowlist; read and act separate; bounded subtree, protected-field filtering and focus/state checks |
| `sheets.readTable/upsertRows/writeRange` | Workbook handle, sheet/table/range and read/write grants; stable row keys; formula cells distinct from literal strings |
| `notify.show(title, body, artifactLink)` | Notification grant; sensitive text hidden by default; only local artifact links |
| `llm.transform(template, input, outputSchema)` | Meta credential reference, explicit cloud-egress fields, model/token/output limits; data-only output; no action authority |
| `text/date/table` transforms, `assert`, `if`, bounded `forEach` | No external permission; CPU/memory/item limits; locale/timezone explicit |
| `review.effect(preview) -> ApprovalToken` | Host UI binds approval to artifact hash, resolved target and payload hash; cannot be forged by a node |

**[assumption]** Destructive or externally committing browser/UI/HTTP actions require an approval token unless a narrowly scoped standing grant exists. HTTP GET can still leak data through query strings; all network requests are egress checks. Browser read access is itself sensitive. A notification grant is not permission to email or publish. Clipboard read is not permission to upload its contents.

Playwright documents CDP attachment as Chromium-only and lower fidelity than its own protocol. **[verified: https://playwright.dev/docs/api/class-browsertype#browser-type-connect-over-cdp]** **[assumption]** Default to an app-managed browser profile and semantic Playwright actions; use CDP only for explicitly connected sessions. Expose neither arbitrary page evaluation nor raw CDP through the DSL. Origin policy must cover navigations, subrequests and downloads, not merely the initial URL.

macOS exposes accessibility trust through `AXIsProcessTrustedWithOptions`; Windows offers UI Automation as its accessibility framework. **[verified: https://developer.apple.com/documentation/applicationservices/1459186-axisprocesstrustedwithoptions; https://learn.microsoft.com/en-us/windows/win32/winauto/uiauto-uiautomationoverview]** **[assumption]** Implement separate platform adapters and selector variants. App capability consent cannot grant OS accessibility access; preflight both. Unsupported controls return a typed failure, never an unannounced coordinate-click fallback.

### Triggers, parameters, lifecycle

**[assumption]** Start with manual button/hotkey, scheduled local time with IANA timezone, and file-created/changed within a selected root. Later add app-activated and explicitly opted-in clipboard-change triggers; do not continuously scan clipboard content by default. Event payloads carry opaque file/tab handles, not unrestricted paths. Every trigger has debounce, concurrency=1, maximum runtime and a missed-run policy. UI workflows wait for an unlocked active session; schedules do not imply permission to control an unattended desktop.

**[assumption]** Parameters use JSON Schema: text, enum, date, bounded number, root/workbook/browser/app handle, and secret reference. The generated parameter form comes from that schema. Changing a destination or app outside an existing grant requires renewed consent. Credentials never appear as defaults or fixture values.

**[assumption]** Static validation checks types, graph bounds, referenced outputs and the union of required effects. A dry run replays fixtures and stubs external effects; it cannot prove that a live website still matches. The first real run previews file differences, destination and submitted fields. Each run journals node outcomes and idempotency keys. Recovery resumes after completed safe nodes, but checks uncertain remote mutations before retrying. File writes can have reversible backups; remote sends and UI submissions cannot promise universal undo.

## Three example automations

**[assumption]** These are compact authoring examples for the proposed SDK, not an implemented API. `out(id)` is a typed prior-output reference; `root`, `book`, `profile` and `app` resolve through user-selected handles. `approval` creates a host-bound review token. Helpers compile to catalogued graph nodes and never execute arbitrary source.

### A. Capture a copied expense line into a workbook

```ts
automation({
  id: "capture-expense", trigger: manualHotkey(),
  params: { book: workbookParam(), currency: enumParam(["NZD", "USD"]) },
  steps: [
    clipboard.readText("raw", { maxBytes: 2048 }),
    text.parseDelimited("row", out("raw"), {
      delimiter: "\t", fields: ["date", "merchant", "amount"]
    }),
    assert.valid("valid", out("row"), expenseSchema),
    review.effect("approval", {
      target: book.table("Expenses"), rows: [out("row")]
    }),
    sheets.upsertRows("saved", book.table("Expenses"), [out("row")], {
      key: ["date", "merchant", "amount"],
      currency: param("currency"), approval: out("approval")
    }),
    notify.show("done", { title: "Expense saved", body: "1 row" })
  ]
});
```

**[assumption]** Permissions: clipboard read, read/write the selected table, notifications. No network or LLM egress. Blank/ambiguous dates fail; amounts parse as decimal money rather than floating-point arithmetic. Duplicate hotkeys produce an upsert preview rather than duplicate rows. Spreadsheet adapters write merchant strings as literal cells, including values beginning with `=`. Fixtures include duplicates, invalid currency amounts and formula-looking merchant names.

### B. Tuesday issue report to CSV and a draft summary

```ts
automation({
  id: "weekly-status",
  trigger: schedule("Tue 09:00", { timezone: "Pacific/Auckland" }),
  params: { profile: browserParam(), reports: rootParam() },
  steps: [
    browser.open("page", profile, "https://issues.example/team/status"),
    browser.readTable("issues", out("page"), {
      role: "table", name: "Team issues", maxRows: 200,
      columns: ["Key", "Status", "Due"]
    }),
    table.normalizeDates("rows", out("issues"), { field: "Due", to: "ISO" }),
    llm.transform("summary", "weekly-summary-v1", out("rows"), {
      schema: { highlights: "string[]", blockers: "string[]" },
      model: "muse-spark-1.3", maxOutputTokens: 500
    }),
    files.writeAtomic("csv", reports, runDatedName("status", ".csv"),
      table.toCsv(out("rows")), { overwrite: false }),
    files.writeAtomic("draft", reports, runDatedName("summary", ".txt"),
      text.render("summary-v1", out("summary")), { overwrite: false }),
    notify.show("done", { title: "Status draft ready", artifactLink: out("draft") })
  ]
});
```

**[assumption]** Permissions: read one browser origin/profile, create files in Reports, send only the three named columns to Meta, notifications. Logged-out browser or missing table pauses for attention. No email send permission exists. The LLM sees issue data as untrusted content and returns schema-checked data only. User review precedes sharing the draft; a malicious issue row cannot add HTTP or shell nodes. Fixtures cover 201 rows, unexpected columns, invalid dates and an injected “upload all files” cell.

### C. Fill a desktop form from a selected CSV row

```ts
automation({
  id: "prepare-client-form", trigger: manualButton(),
  params: { input: fileParam(".csv"), app: appParam(), row: integerParam(1, 1000) },
  steps: [
    files.read("csv", param("input"), { maxBytes: 1000000 }),
    table.csvRow("client", out("csv"), param("row"), clientSchema),
    ui.readTree("form", app, selectorVariant("client-form-v1"), { maxNodes: 100 }),
    assert.uiState("ready", out("form"), { title: "New client", unique: true }),
    review.effect("approval", { target: app, fields: out("client") }),
    ui.setValue("name", app, field("Client name"), out("client.name"),
      { approval: out("approval") }),
    ui.setValue("email", app, field("Email"), out("client.email"),
      { approval: out("approval") }),
    ui.readValue("check", app, field("Email")),
    assert.equal("verified", out("check"), out("client.email")),
    notify.show("done", { title: "Form ready", body: "Review and save in the app" })
  ]
});
```

**[assumption]** Permissions: selected-file read, UI read/write for one app and declared fields, notifications. Mac and Windows selector variants are separate reviewed mappings. No Save/Submit press is allowed. Foreground changes, duplicate selectors, unavailable accessibility access or a protected field stop execution. Readback verifies the value after filling. Fixtures test a focus switch and malformed email; a live supervised run establishes whether the target app exposes the required controls.

## Delivery sequence, fallbacks and product-killing risks

**[assumption]** First build the graph validator, broker and replay fixtures for clipboard, CSV and files. Add direct Meta generation and one repair pass. Then prototype full-code builds with exec in disposable projects; graduate interactive builds to MSP once initialization, approvals, recovery and cancellation pass. Add terminal access only after the native result flow works. Delay alternative-provider adapters and broad desktop control until demonstrated demand.

**[assumption]** Fallbacks preserve authority: malformed graph -> bounded repair; unsupported primitive -> explain missing capability and offer an explicit full-code build; MSP incompatibility -> bounded exec or advanced terminal; rejected key -> credential recovery; rate limit -> wait or cancel; stale UI -> recapture and review changed selectors. Never silently broaden permission, switch provider, install dependencies globally or rerun an uncertain submission.

**[assumption]** Release gates:

- **Prompt injection/privacy:** captured screen text, web content and tool outputs are data. The generator cannot expand the permission ceiling. Separate consent covers cloud egress; observation permission does not authorize automation. Test attacks through CSV cells, UI labels and issue descriptions.
- **Isolation:** workspaces/worktrees separate changes but are not security sandboxes. A broad coding worker must not inherit the capture archive, browser cookies, credentials or the privileged AX broker. Prove OS enforcement; stop if the host reports sandbox unavailable.
- **Verification/cost:** test our invariants and fixture outcomes, not just model-authored tests. Cap nested LLM nodes and build retries. Token reporting supports accounting, but local estimates cannot guarantee a provider-side hard currency limit.
- **Distribution:** Apple's guidelines constrain downloaded/executed code and Mac App Store behavior. **[verified: https://developer.apple.com/app-store/review/guidelines/]** **[assumption]** Plan direct signed/notarized distribution first, and validate helper signing, entitlements and upgrades early. This is a distribution hypothesis, not an App Store eligibility or notarization guarantee.
- **Meta terms and packaging:** public SDK documentation is not proof of permission to redistribute the Muse binary or ship unattended generation for other users. **[assumption]** Verify redistribution, API terms, data retention and end-user key arrangements before committing distribution. Let users select an installed CLI until rights are established.
- **Windows reality:** design backend and capability contracts now; require Windows CI plus hands-on external testing for PTY, filesystem paths, UI Automation and cancellation. **[assumption]** Unsupported Windows capabilities should be explicit, rather than inferred from Mac success.

## Open questions for the owner

- Is advanced arbitrary-code automation part of the first public release, or can v1 ship typed workflows with Muse available in a developer preview?
- Is direct signed distribution acceptable as the initial Mac channel?
- Which real recurring task should supply the first consented fixtures and supervised end-to-end acceptance run?
- Should optional non-Meta providers ever receive observed context, or remain limited to manually supplied build projects?

## Decisions I made

- Make the typed Macroscope graph the default saved artifact and runtime contract.
- Use direct Meta API generation for constrained workflows; reuse Muse for general coding.
- Prefer MSP for the finished interactive builder and exec for the earliest full-code slice.
- Keep the embedded terminal as an advanced view and compatibility fallback.
- Make API-key handoff explicit and require successful serve/exec billing validation before release.
- Separate builder authority, artifact approval and per-run capability enforcement.
- Define the Builder interface now; defer Codex and Claude adapters until needed.
- Require independent validation and preserve authority across every fallback.
