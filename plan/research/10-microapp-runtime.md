# 10. Micro-app runtime, lifecycle and reliability

Track owner: planning agent 10 (Claude). Scope: what a generated micro-app *is*, how it is
packaged, triggered, executed, tested, repaired and eventually shared. Related tracks I
lean on but do not own: Muse/MSP embedding (04), privacy and sandboxing (08), pattern
engine and the Opportunity object (09), architecture and process model (11/12).

Fact markers: **[verified: URL]**, **[local: checked on machine]**, **[assumption]**.

---

## 0. TL;DR (the decisions)

1. **A micro-app is a small git repo plus a manifest** (`macroscope.yaml`). It lives in
   Macroscope's data directory, is versioned by commits, and is only ever executed by
   Macroscope's **executor**, never directly by the OS.
2. **One primary artifact type for v1: a TypeScript "workflow" on the Macroscope SDK,
   run in a bundled, pinned Deno.** Every side effect (file write, browser click, Mail
   draft, clipboard, secret, notification, runtime LLM call) goes through an **SDK call to
   a host-side broker**, not directly to the OS. That one decision is what makes
   permissions, dry-run previews, undo, logging and self-repair possible.
3. Deno is launched as a **pure compute sandbox**: read access only to declared paths,
   write access only to a per-run scratch dir, **no network, no subprocess, no env, no
   FFI**. All real I/O is brokered. Python (via a bundled `uv`) is a v1.5 secondary
   runtime under an OS sandbox profile, using the same broker protocol.
4. **Browser automation runs in a Macroscope-managed browser profile** (Chrome for
   Testing driven by Playwright in a privileged broker sidecar), not the user's everyday
   Chrome profile. Chrome 136+ refuses remote debugging on the default profile anyway.
   Acting inside the user's real browser comes later via the companion extension.
5. **Every run has a plan.** The SDK has `plan`, `replay` and `live` modes. "Preview what
   it will do" means running the workflow in plan mode (reads are real, writes are
   simulated) and showing the resulting effect list.
6. **Undo is part of the design**: file writes snapshot the previous version, deletes go
   to the Trash/Recycle Bin, and every effect is recorded in a journal with an inverse
   where one exists. Irreversible effects (sending an email, a non-idempotent POST) are
   labeled as irreversible and gated.
7. **The user's own episode is the answer key.** Tests come from what the pattern engine
   observed (inputs, the artifact the user produced by hand). Macroscope writes the
   **locked tests**, which the agent cannot edit, and the agent must make them pass
   before the app is offered.
8. **Expect automations to break, and plan for repair.** Runs are classified by failure
   type, and each failure type has a defined action. Repairs use `session/fork` of the build
   session plus the failing run's journal and a fresh screenshot. Each failing run becomes
   a new regression test. Rollback is a pointer flip. A circuit breaker pauses an app
   after 3 consecutive failures.
9. **Trust is earned per app**: `supervised` → `confirm-irreversible` → `unattended`.
   Nothing that widens permissions or sends something irreversible is ever auto-promoted.
10. Sharing is phase 3. Export as a standalone script/binary (via `deno compile`) is phase 2.
    Fixtures, which hold the user's data, are never exported.

---

## 1. What a micro-app is

### 1.1 Artifact types, ranked for v1

| Rank | Artifact | v1 status | Why | Main risk |
|---|---|---|---|---|
| 1 | **Workflow on Macroscope SDK (TypeScript, Deno)** | **P0, the default** | LLMs write TS well. The SDK gives us choke points for permissions, plan/undo and logging. One runtime to ship. | The SDK's surface becomes our maintenance burden. |
| 2 | **Auto-generated form panel** (inputs schema → form in a Macroscope webview) | **P0** (comes free with #1) | Most micro-apps need 1–3 parameters. The form is rendered from the manifest, so the agent writes no UI code. | none worth noting |
| 3 | **OS scripting as SDK steps**: AppleScript/JXA via `osascript`, `shortcuts run`, PowerShell on Windows | **P0 on macOS** (Mail, Finder, Numbers, Excel, Calendar), P1 on Windows | Native apps are where the browser can't reach. They are wrapped as brokered effects (`mac.applescript(...)`), not free-standing scripts, so they are still permissioned and journaled. | Apple Events TCC prompts. AppleScript dictionaries vary by app. |
| 4 | **Browser automation** (Playwright against a managed profile) | **P1** (v1 but after the first 3) | Most repetitive knowledge work is in web apps. | It breaks the most often. Login/2FA friction. Bot detection. |
| 5 | **Python script** (via `uv`, PEP 723 inline deps) | **P1.5** | Excel/PDF/pandas-heavy jobs where Python libs are clearly better (openpyxl, pdfplumber). | Weaker sandbox story. ~50 MB lazy download. |
| 6 | **Custom UI panel** (agent-written HTML/TS in a Macroscope-hosted webview, talks to the SDK via postMessage) | **P2** | Needed for "review these 12 rows then submit" style tools. | Webview security (CSP, no remote content). |
| 7 | **Menu-bar / tray items** | **P2** | Mostly a *surface* for apps (pinned apps in Macroscope's own tray menu, plus an optional dynamic title such as "3 invoices waiting"). Never a separate process. | Clutter |
| 8 | **Generated browser extension** | **Not in v1. Maybe never as a standalone artifact.** | Dev-mode extension install is user-hostile, and store review takes days. The better route is the Macroscope companion extension (track 06) running agent-written content scripts via `chrome.userScripts`. | MV3 userScripts needs a user toggle. Store policy. |

The ranking does not say "scripts are second-class". A plain TS script is just a
workflow that happens to use few SDK effects. There is **no separate YAML DSL**. I
considered one (declarative step list, easy to visualize) and rejected it for v1. Models
are much better at TypeScript than at a bespoke DSL, and a DSL needs an escape hatch by
week two anyway. We still get a visual step list for free, because the plan-mode effect
journal *is* that list.

### 1.2 On-disk layout

```
~/Library/Application Support/Macroscope/          (Windows: %LOCALAPPDATA%\Macroscope\)
  runtimes/
    deno/2.x.y/deno                 pinned, checksummed, signed with our Developer ID
    uv/0.x.y/uv                     pinned
    python/cpython-3.12.x/          lazy: first Python app only
    chrome-for-testing/1xx.0.x/     lazy: first browser app only
  sdk/
    macroscope-sdk-1.4.0/           TS SDK + Python SDK, vendored, versioned
  browser-profiles/
    work/                           managed Chromium user-data-dir (encrypted volume, see 08)
  apps/
    app_01JB7Q4M2X9T3K/             one git repo per micro-app
      macroscope.yaml               the manifest
      src/main.ts
      src/lib/*.ts
      tests/locked/                 written by Macroscope from the episode; agent read-only
      tests/agent/                  written by the agent
      deno.lock
      README.md                     generated plain-English explanation
      .gitignore                    ignores fixtures/ and .mscope/
      fixtures/                     PERSONAL DATA: encrypted at rest, never exported
      .mscope/
        runs/<run_id>/journal.jsonl
        runs/<run_id>/log.txt
        runs/<run_id>/screens/      step screenshots (browser apps only)
        runs/<run_id>/undo/         pre-images for undo
        current -> refs/tags/v7     pointer to the active version
```

Why use a git repo for each app: the Muse build agent works natively in a repo, and so does
`--worktree` (track 04). Versions, diffs and rollbacks come for free. A repair is just a
branch. Cost: one `.git` per app, which is a few hundred KB. Fine.

### 1.3 Process model (the runtime slice)

```
 +---------------------------- Macroscope app ------------------------------+
 |  UI (plan cards, run log, health)      Scheduler / trigger router         |
 +------------------------------+-------------------------------------------+
                                | IPC (track 11/12)
 +------------------------------v-------------------------------------------+
 |  EXECUTOR SERVICE (Rust)                                                  |
 |   - run queue, per-app mutex, global concurrency (3)                      |
 |   - spawns runs, enforces timeouts / RSS caps / output caps               |
 |   - BROKER: implements every SDK effect, checks manifest permissions,     |
 |     writes journal, captures undo pre-images, scrubs secrets from logs    |
 |       |-- fs / trash handler        |-- keychain / credential mgr        |
 |       |-- clipboard handler         |-- notifications / ui.ask (panels)  |
 |       |-- osascript / shortcuts / powershell handler                     |
 |       |-- http handler (host allowlist, secret egress binding)           |
 |       |-- llm handler (Muse/Meta API, shared key + meter, track 04)      |
 |       '-- browser handler ---> BROWSER SIDECAR (privileged, Playwright)   |
 |                                   '-> Chrome for Testing, managed profile |
 +-------------+-------------------------------------------------------------+
               | spawn per run;  JSON-RPC 2.0 over fd 3 (ndjson)
               | stdout/stderr = free-form log lines
 +-------------v--------------------+    +-----------------------------------+
 | deno run --no-prompt             |    | python (uv run --offline)        |
 |   --allow-read=<app>,<decl paths>|    |  under sandbox-exec profile (mac)|
 |   --allow-write=<run scratch>    |    |  / restricted job (win)          |
 |   (no net/run/env/ffi/sys)       |    |  same SDK protocol on fd 3       |
 |   src/main.ts                    |    +-----------------------------------+
 +----------------------------------+
```

The micro-app process gets nothing except "compute + talk to the broker". Even if
generated code were malicious, or got prompt-injected by data it reads, the most it can do
is ask the broker for effects that the manifest already allows, and every such request is
journaled. This holds the line track 08 needs (capability-scoped manifests), and it stops
depending on the honesty of the code.

---

## 2. The SDK in one screen

The SDK is the contract the Muse "macroscope-microapp" skill teaches (track 04). It is small
on purpose:

```ts
import { app, fs, browser, mac, clipboard, http, secrets, llm, ui, expect, log }
  from "macroscope:sdk";            // import-mapped to the vendored SDK version

export default app(async (ctx) => {
  const { sprint, recipients } = ctx.inputs;          // validated against manifest

  const page = await browser.open("https://acme.atlassian.net/jira/software/c/projects/ACME/boards/12",
                                  { profile: "work" });
  const rows = await page.table({ role: "grid", name: /Sprint/ })   // semantic locator
                         .rows(["Key", "Status", "Assignee", "Due"]);
  expect(rows.length, "issues on board").toBeGreaterThan(0);        // postcondition

  const csv = toCsv(rows.map(r => ({ ...r, Due: isoDate(r.Due) })));  // pure, unit-tested
  const out = await fs.writeText(`~/Documents/Status/status-${ctx.today}.csv`, csv);

  await mac.mail.createDraft({                        // brokered AppleScript recipe
    to: recipients, subject: `Status ${sprint}`, body: summarize(rows), attach: [out],
  });                                                 // draft only: reversible
  return { rows: rows.length, file: out };            // run result shown in the UI
});
```

Design rules for the SDK:

- **Every effect is a named, typed call** (`fs.writeText`, `browser.click`,
  `mac.mail.createDraft`, `http.post`). Each effect declares `{reads|writes, reversible,
  idempotent}` metadata in the SDK, so the planner and the risk tiering never have to guess.
- **Pure logic is plain code.** Transforms like `toCsv` and `isoDate` are ordinary functions
  that the locked tests target directly.
- **`expect(...)` postconditions** are first-class. They are how we catch silent breakage
  ("0 rows" is not success).
- **Locators are semantic first** (role + accessible name + text), with fallbacks
  recorded at build time (section 7.3).
- High-level recipes (`mac.mail.createDraft`, `mac.numbers.readSheet`,
  `win.outlook.createDraft`) are written and tested **by us**, not generated. The agent
  composes them. This is where most native-app reliability comes from, and it is also the
  biggest ongoing workload for the owner: start with ~10 recipes for the most common apps
  seen in the owner's own captures.
- The escape hatch is `mac.applescript(source)` / `win.powershell(source)`, which is a raw
  script with a declared target app. It is allowed, but it raises the risk tier and is
  always shown in full in the review UI.

---

## 3. The manifest

YAML on disk (humans and agents read it). Validated against a JSON Schema that ships with
the SDK (`macroscope.app/v1`). The executor **never trusts** fields it can compute itself
(risk tier, irreversible effects). It recomputes them from the code plus a plan run, and
refuses to run on a mismatch.

### 3.1 Full example: the brief's "Tuesday Jira status" workflow

```yaml
schema: macroscope.app/v1
id: app_01JB7Q4M2X9T3K              # ULID; stable across versions
slug: jira-tuesday-status
name: Tuesday Jira status
description: >
  Reads the ACME sprint board in Jira, writes status-YYYY-MM-DD.csv to
  ~/Documents/Status and creates (does not send) the status email draft in Mail.
icon: sf:tablecells                  # SF Symbol on mac, Fluent icon name mapped on Windows
version: 1.3.0                       # bumped by the executor, not the agent
build: 7                             # monotonically increasing; git tag v7
kind: workflow                       # workflow | python | panel
surfaces: [launcher, tray]           # where it shows up: launcher (Cmd-K), tray menu, panel

runtime:
  engine: deno
  engine_version: "2.x.y"            # exact version bundled when this build was tested
  sdk: "1.4"                         # SDK minor; executor maps to a vendored version
  entrypoint: src/main.ts
  lockfile: deno.lock
  platforms: [macos]                 # mac.* recipes used -> macOS only

inputs:                              # JSON-Schema-ish; drives the auto form
  - id: sprint
    type: string
    label: Sprint name
    default: "current"
  - id: recipients
    type: email[]
    label: Send draft to
    default: ["team-leads@acme.example"]
  - id: include_done
    type: boolean
    label: Include issues already Done
    default: false

triggers:
  - type: manual                     # launcher, tray, app card "Run"
  - type: hotkey
    keys: { macos: "Ctrl+Opt+Cmd+J", windows: "Ctrl+Alt+Shift+J" }
  - type: schedule
    cron: "0 14 * * TUE"
    timezone: local
    on_missed: run_once_when_awake   # | skip
    mode: confirm                    # notification -> plan card -> Run
  - type: context                    # "when I'm in app X"
    when:
      app: { macos: com.google.Chrome, windows: chrome.exe }
      url_matches: "https://acme.atlassian.net/jira/*"
    action: suggest                  # shows a chip in the HUD; never auto-runs

permissions:
  fs:
    read:  ["~/Documents/Status/"]
    write: ["~/Documents/Status/status-*.csv"]
    delete: []                       # deletes always go to Trash anyway
  network: []                        # no direct HTTP; Jira is reached through the browser
  browser:
    profile: work
    origins: ["https://acme.atlassian.net"]
    mutating: false                  # only reads pages; no form submits
  apps:
    - id: { macos: com.apple.mail }
      via: recipe:mac.mail
      verbs: [create_draft]
  clipboard: { read: false, write: false }
  secrets: []                        # e.g. [{ name: jira_api_token, hosts: [acme.atlassian.net] }]
  llm:
    model: muse-spark-1.3            # model id from muse catalog [local]
    max_tokens_per_run: 4000         # used by summarize(); billed to owner's Meta key
  notifications: true

limits:
  timeout_s: 120
  memory_mb: 512
  max_effects: 300
  max_log_kb: 2048

trust:
  level: supervised                  # supervised | confirm-irreversible | unattended
  promote_after_clean_runs: 3

tests:
  command: macroscope test           # wraps `deno test` with SDK in replay mode
  locked:
    - tests/locked/transform.test.ts # toCsv/isoDate against the user's own CSV
    - tests/locked/plan.golden.json  # expected effect sequence in plan mode
  agent: ["tests/agent/**/*.test.ts"]
  fixtures: fixtures/ep_2026-10-14/  # recorded page snapshots, HAR, Mail draft text
  locked_digest: sha256:9c1e...      # executor checks this; agent cannot edit locked tests

provenance:
  opportunity_id: opp_01JB6ZP0VQ2M
  episodes: [ep_01JAZ..., ep_01JB2..., ep_01JB5...]   # the 3 Tuesdays observed
  build:
    agent: muse-code 1.4.11          # [local]
    model: muse-wonder-1.0           # [local] default model observed in muse exec
    reasoning_effort: high
    msp_schema_fingerprint: "sha256:f282ad16e21520d0cda68cb190149b86e2b062eef1dd282749360c05aa3d1082"  # [local]
    session_id: 0192c7f0-6a3e-7c55-9e1a-1f0b6c1d2a77
    iterations: 4                    # test-fix cycles before green
    tokens: { input: 182340, output: 21455 }
  history:
    - { build: 5, reason: initial, at: 2026-10-15T09:12:00Z }
    - { build: 6, reason: "repair: board column renamed 'Due date'->'Due'", at: 2026-10-21T14:03:00Z, auto: true }
    - { build: 7, reason: "user edit: add include_done input", at: 2026-10-22T08:40:00Z }
  approved_by_user_at: 2026-10-22T08:41:10Z

integrity:
  tree_digest: sha256:4be0...        # git tree hash of the approved commit
  signature: ed25519:device-key:...  # signed on approval; executor refuses unsigned edits
```

### 3.2 Smaller example: a file-watch app (shows the other trigger types)

```yaml
schema: macroscope.app/v1
id: app_01JB8A0F5K2N7C
name: File Downloads invoices
version: 1.0.0
build: 2
kind: workflow
runtime: { engine: deno, engine_version: "2.x.y", sdk: "1.4", entrypoint: src/main.ts, platforms: [macos, windows] }
triggers:
  - type: file_watch
    path: "~/Downloads"
    glob: "*invoice*.pdf"
    events: [created]
    debounce_ms: 3000                # wait for the download to finish
    mode: unattended                 # earned after 3 clean supervised runs
  - type: app_event                  # also run when the accounting app is opened
    event: app_activated
    app: { macos: com.xero.desktop, windows: Xero.exe }
    mode: suggest
permissions:
  fs:
    read:  ["~/Downloads/"]
    write: ["~/Documents/Finance/Invoices/{yyyy}/"]
    move_from: ["~/Downloads/*invoice*.pdf"]   # move = reversible (journaled)
trust: { level: unattended }
```

### 3.3 Trigger semantics

| Trigger | Implementation (mac / win) | Auto-run allowed? |
|---|---|---|
| `manual` | launcher, tray menu, app card, CLI `macroscope run <slug>` | yes (it's the user) |
| `hotkey` | Carbon `RegisterEventHotKey` / `RegisterHotKey` [assumption: standard APIs] | yes |
| `schedule` | executor's own scheduler (cron + tz), **not** launchd/Task Scheduler in v1 | only at trust ≥ confirm-irreversible; otherwise notification → plan card |
| `file_watch` | FSEvents / ReadDirectoryChangesW (via Rust `notify` crate) + debounce | same rule |
| `app_event` | from the capture layer's app-focus/window events (tracks 06/07) | `suggest` by default |
| `context` | capture layer state: frontmost app + URL/title match | **never** auto-run; shows a chip |

Why use our own scheduler instead of the OS's: one place to log, pause, and apply the
circuit breaker, and no extra per-OS persistence surface that looks like malware to AV
(track 08). Trade-off: schedules don't fire while Macroscope isn't running. It is a login
item anyway, and `on_missed: run_once_when_awake` covers sleep.

---

## 4. Runtimes to bundle

### 4.1 Decision: Deno (bundled, pinned) as the primary runtime

- **Deno has a real default-deny permission model** with scoped allow flags
  (`--allow-read=<paths>`, `--allow-net=<hosts>`, `--allow-env=<vars>`, `--allow-run`)
  and `--deny-*` flags that override allows. A refused operation throws `NotCapable`
  **[verified: https://docs.deno.com/runtime/reference/permissions/ ,
  https://docs.deno.com/runtime/fundamentals/security.md]**. Native TypeScript means no
  build step. `npm:` and `jsr:` specifiers give access to the library ecosystem.
- **Bun has no permission/sandbox model** as of mid-2026, per a secondary source
  **[verified (secondary): https://www.safeguard.sh/resources/blog/nodejs-deno-bun-security-model-comparison]**.
  Rejected for running generated code.
- **Node's permission model is now marked Stable** (`--permission`, `--allow-fs-read`,
  `--allow-child-process`, etc.), and v26 adds an audit mode
  **[verified: https://nodejs.org/api/permissions.html ,
  https://r2.nodejs.org/docs/latest/api/permissions.html]**. It's a credible fallback,
  but Deno's model is older, scoped per host/path for every category, and TS-native.
  We would choose Node only if Deno + Playwright compatibility fails (4.3).

Since the broker does all I/O, the actual Deno flags are minimal:

```
deno run --no-prompt --no-remote --cached-only --lock=deno.lock --frozen
  --allow-read=<app dir>,<sdk dir>,<manifest fs.read paths>
  --allow-write=<.mscope/runs/<run_id>/scratch>
  --v8-flags=--max-old-space-size=<limits.memory_mb>
  --deny-net --deny-run --deny-env --deny-ffi --deny-sys
  src/main.ts            (fd 3 = broker channel)
```

(`--no-remote`/`--cached-only`/`--frozen` keep a run from fetching code. Dependencies are
resolved once at build time into a per-app vendored cache. Exact flag names must be
checked against the pinned Deno version **[assumption]**.)

Even manifest-declared fs **reads** are given to Deno directly, but **writes** are not. That
forces every write through `fs.*` in the SDK, which in turn makes trash-not-delete and undo
pre-images guaranteed, not optional.

### 4.2 Python via uv (v1.5)

- Bundle the `uv` binary (pinned). On the first Python app, after the user consents,
  `uv python install` fetches a python-build-standalone CPython into
  `runtimes/python/` (`UV_PYTHON_INSTALL_DIR`)
  **[verified (mirror of uv docs): https://mintlify.com/astral-sh/uv/cli/python-install]**.
  `uv 0.12.5` is on the owner's Mac **[local]**, but we ship our own copy and never use the
  user's.
- Scripts use PEP 723 inline metadata with exact pins. `uv lock --script` runs at build time,
  and `uv run --offline --frozen` runs at run time, so no network is needed when a run happens
  **[assumption: flag spellings to check against the pinned uv]**.
- Python has no in-language permission system, so it runs under an OS sandbox. On macOS
  that means a generated `sandbox-exec` profile: deny network, deny file-write except
  scratch, allow file-read on declared paths. `sandbox-exec` is deprecated but still works;
  track 08 owns that call. On Windows it runs in a Job Object with a low-integrity/restricted
  token, with AppContainer later. Same fd-3 broker protocol, with a thin Python SDK.

### 4.3 Browser sidecar

The browser handler is a **privileged** sidecar. It is *not* the micro-app process. It runs
Playwright (`playwright-core`) and drives a pinned **Chrome for Testing**, downloaded lazily
(~150–250 MB **[assumption]**). It could run on our bundled Deno with full permissions
(**[assumption: Deno-Playwright compat must be spiked in week 1]**). If that spike fails,
we bundle a pinned Node just for the sidecar. The micro-app never receives a CDP endpoint
or cookies. It only sends `browser.click({role, name})` style requests, which the broker checks
against `permissions.browser.origins`.

### 4.4 No toolchains for the user

- Deno, uv and the SDK are inside the signed app bundle, signed with our Developer ID
  under hardened runtime. V8 likely needs the `com.apple.security.cs.allow-jit` /
  `allow-unsigned-executable-memory` entitlements on the helper **[assumption, verify
  during notarization spike]**. Python and Chrome for Testing are lazy, checksummed
  downloads into our data dir, and their binaries are verified against a manifest we sign.
- `osascript` and `shortcuts` are built into macOS (`/usr/bin/osascript`,
  `/usr/bin/shortcuts` **[local]**). `shortcuts run "<name>" -i <file> -o <file>` is the
  supported CLI. Shortcuts that prompt for input hang a headless run
  **[verified: https://support.apple.com/guide/shortcuts-mac/apd455c82f02/mac]**, so the
  broker always applies a timeout and never calls a Shortcut that contains "Ask for Input".
- On Windows, use the inbox **Windows PowerShell 5.1** (`powershell.exe -NoProfile
  -NonInteractive -ExecutionPolicy Bypass -File`) so pwsh 7 is not required
  **[assumption: 5.1 inbox on Win10/11]**.

### 4.5 Version pinning and runtime upgrades

- Each manifest records the exact `engine_version` it was tested on and the SDK minor.
- A Macroscope release ships *one* new Deno version and **keeps old versions while any app
  references them**.
- When a runtime or SDK is upgraded, a **compat sweep** runs: every app's tests run in
  replay mode on the new runtime in the background. Apps that pass get migrated (their
  manifest is bumped, a new build is tagged). Apps that fail stay pinned and get a "needs
  attention" badge. That's the same machinery as section 7.

---

## 5. Execution

### 5.1 Run lifecycle

```
 trigger --> QUEUED --> PREFLIGHT --------> PLANNING --> AWAITING_APPROVAL --+
                         | manifest valid?     | plan mode run               |
                         | signature ok?       | effect list + diffs         | user: Run / Edit / Cancel
                         | inputs valid?       |                             | (skipped when trust allows)
                         | runtime present?    v                             v
                         |                  PLAN_FAILED ---> repair?     EXECUTING (live)
                         v                                                   |
                      BLOCKED (permission revoked, auth needed)              v
                                                                         VERIFYING (postconditions,
                                                                          output schema)
                                                              +--------------+--------------+
                                                              v              v              v
                                                          SUCCEEDED   FAILED(class)   PARTIAL -> offer Undo
```

- **One run per app at a time** (per-app mutex). Global concurrency is 3. Scheduled runs that
  overlap are coalesced.
- **Timeouts**: default 120 s wall clock, max 15 min. A per-effect timeout comes from the SDK
  (browser actions 30 s, AppleScript 20 s). On timeout: SIGTERM, then SIGKILL after 3 s, then
  the sidecar's browser context is closed.
- **Resource caps**:
  - macOS: V8 heap flag, plus an executor watchdog that polls RSS (`proc_pidinfo`) every
    500 ms and kills the process above `memory_mb` × 1.5. `RLIMIT_CPU` via `setrlimit`. There
    are no cgroups on macOS, so this is best-effort **[assumption]**.
  - Windows: Job Object with `JOB_OBJECT_LIMIT_PROCESS_MEMORY`, CPU rate control and
    kill-on-close. That is stronger than macOS.
- **Output caps**: stdout/stderr truncated at `max_log_kb`. Effect count capped at
  `max_effects`, which also catches runaway loops.
- **Kill switch**: a global "Stop all automations" item in the tray, and per-app pause.
  Both cancel in-flight runs immediately.

### 5.2 Logging and the journal

Each run produces:

- `journal.jsonl`: one line per effect:
  `{seq, ts, effect:"fs.writeText", args_redacted, target, mode:"live", result_digest,
  reversible:true, undo:{kind:"restore", preimage:"undo/0003.bin"}, duration_ms}`.
- `log.txt`: the app's own `log.*` and stdout lines.
- `screens/`: a screenshot after each browser action, which the repair loop needs. Retention
  is 14 days, or the last 10 runs, whichever is larger. Screenshots are taken of our managed
  browser only, never the user's desktop.
- A row in SQLite (`runs` table: app_id, build, trigger, status, failure_class,
  duration, effects_count, tokens, started_at), which feeds the health dashboard.

**Secret scrubbing**: when the broker hands a secret to the app, it registers the value.
Every log/journal write is scrubbed of registered values. **Secret egress binding**: an
`http.*` request that carries a registered secret to a host outside that secret's
`hosts` list is blocked and logged. That covers the main way a prompt-injected or buggy app
could leak a token.

### 5.3 Plan mode: "preview what it will do"

The SDK runs in one of three modes, chosen by the executor:

| Mode | Read effects | Write effects | Used for |
|---|---|---|---|
| `plan` | **real** (read the board, read files) | **simulated**: recorded, return synthetic results (`fs.writeText` returns the path; `createDraft` returns `draft:sim-1`) | preview card, golden-plan tests |
| `replay` | served from fixtures/cassettes | simulated | all automated tests, compat sweep, repair validation |
| `live` | real | real, journaled, undo pre-images captured | actual runs |

Browser apps in plan mode run **up to, but not including, the first effect marked
`mutating`** (a submit, save or send). The plan card shows the screenshot at that point:
"I will click **Submit** on this form." Code after a simulated mutation keeps running with
synthetic results. Branches that depend on real write results are shown as "then: …
(depends on result)".

Plan card (what the user sees on a supervised run):

```
+-- Tuesday Jira status - build 7 ------------------------------ [supervised] --+
|  Read   Jira board ACME / Sprint 42 (acme.atlassian.net)    17 issues        |
|  Write  ~/Documents/Status/status-2026-10-22.csv            NEW, 18 lines    |
|         [preview table]                                                      |
|  Draft  Mail: "Status Sprint 42" to team-leads@acme.example  NOT SENT        |
|         [preview text]                                                       |
|  AI     summarize 17 rows (muse-spark-1.3, ~1.2k tokens)                     |
|                                                                              |
|  Nothing irreversible. Undo available for 7 days.                            |
|                               [ Edit inputs ]  [ Cancel ]  [ Run  (Enter) ]  |
+------------------------------------------------------------------------------+
```

Plan mode costs one extra run of the read side. For schedule/file-watch runs at
`unattended` trust, we skip the plan and keep the journal and undo.

### 5.4 Undo

| Effect | Inverse | Notes |
|---|---|---|
| `fs.write*` over existing file | restore pre-image | pre-image stored in `undo/` |
| `fs.write*` new file | move new file to Trash | |
| `fs.delete` | **never unlink**: move to Trash (`NSFileManager trashItemAtURL` / `IFileOperation` with `FOFX_RECYCLEONDELETE`) **[assumption: standard APIs]**; undo = restore from Trash | |
| `fs.move/rename` | reverse move | conflict check |
| `clipboard.write` | restore previous clipboard | only within 10 min |
| `mac.mail.createDraft` | delete the draft (recipe) | |
| `browser.*` mutating, `http.post/put/delete`, `mail.send`, `shortcuts.run` | **irreversible** | gated by trust rules |

"Undo this run" walks the journal in reverse. Before each inverse step it compares the
target's current hash with the post-image hash. If something else changed the file in the
meantime, it stops and asks. The undo window is 7 days, after which pre-images are pruned.

---

## 6. Testing generated tools

### 6.1 Where tests come from

The pattern engine (track 09) hands the builder an **Opportunity** with observed steps,
sample inputs/outputs and evidence frames. For testing, the key insight is: **the user
already did this task by hand, at least 3 times, so we have real input/output pairs.**

| Test asset | Source | Who writes it | Editable by agent? |
|---|---|---|---|
| **Golden output** | the artifact the user produced (the CSV they saved, the email they wrote), found via file events and the episode | Macroscope, with user consent to use the real content as a local fixture | no (locked) |
| **Input fixtures** | what the user read during the episode: a DOM snapshot + HAR of the Jira page from the managed browser at build time, or a saved copy of the source file | Macroscope (capture at build time) | no (locked) |
| **Golden plan** | the expected effect sequence, derived from the observed action trace ("read board → write csv in ~/Documents/Status → Mail draft") | Macroscope generates it, **user confirms it** in the build UI ("Is this what you do?") | no (locked) |
| Unit tests for transforms | agent | agent | yes |
| Edge-case tests | agent (empty board, weird dates) | agent | yes |

Comparing golden output is structural, not byte-exact: CSV is compared by parsed rows and
column order, and dates are normalized. For prose such as email bodies, check required fields
(recipients, subject pattern, every issue key mentioned) rather than wording. An LLM-judge
check is optional and off by default (it costs tokens and adds noise).

Episode data is personal. Fixtures stay in `fixtures/`, encrypted at rest (track 08),
gitignored, and are never exported or sent anywhere except to the build agent's model
context, and only in redacted form, governed by the "what was sent" log (track 08).

### 6.2 Replay

- **HTTP**: the broker's `http` handler records and replays cassettes (method + URL +
  body hash).
- **Browser**: Playwright `routeFromHAR` replays network from the recorded HAR, plus a
  saved DOM snapshot for apps that render client-side **[assumption: feasible for most
  SaaS, with flaky cases]**. Browser replay tests are allowed to be marked
  `@live-only` if replay is impossible. Then the gate needs a supervised live plan-mode
  run instead (section 6.3, step 6).
- **Native app recipes** (Mail, Numbers): mocked at the recipe boundary. Recipes have
  their own live tests in *our* CI, so generated apps only test that they call the recipe
  correctly.
- **fs**: a temp dir seeded from fixtures. Paths are rewritten through the SDK's path
  mapper.

### 6.3 The gate: nothing is offered until it's green

```
 Opportunity --> [Macroscope writes locked tests + golden plan + fixtures]
                     |
                     v
              Muse build session (track 04) in the app repo
                     |
          +----------v-----------+   fail: test output + journal fed back
          |  agent edits src/    |<--------------------------------------+
          |  runs `macroscope    |                                       |
          |  test` via shell     |---------------------------------------+
          +----------+-----------+   (max 6 iterations, token budget cap)
                     | agent says done
                     v
         EXECUTOR-SIDE VERIFICATION (agent cannot influence)
           1. manifest validates; locked_digest matches
           2. `deno check` + lint; static scan (no dynamic import, no eval, no
              Deno.* I/O APIs, no raw fetch -- all denied anyway, but fail early)
           3. locked tests + agent tests pass in replay mode, in a fresh process
           4. plan-mode run on fixtures == golden plan (effect kinds, targets, order)
           5. permission diff <= what the Opportunity justified (a new host or app
              not seen in the episode = flag to user, never silent)
           6. browser/native apps: one supervised live plan-mode run with the user
              watching ("dry run on your real Jira")
                     |
                     v
              OFFERED: review screen (plain-English README, permissions, plan card,
              code diff for power users) -> user approves -> signed, tagged build
```

The agent can't edit locked tests. The executor verifies `locked_digest` and runs tests
itself instead of trusting the agent's claim, which closes the obvious failure where a
model "passes" by weakening tests. If 6 iterations don't reach green, the build fails
honestly. The UI shows what blocked it and offers "try another approach" (MSP
`session/fork`, track 04) or "make it simpler" (drop the hardest step and leave it manual).

---

## 7. Reliability: detecting breakage and repairing it

UIs change, auth expires, and CSV columns get renamed. Assume every browser automation
breaks at some point within months **[assumption, from industry RPA experience]**. The product
promise is not "never breaks". It is "tells you right away, usually fixes itself, never
silently does the wrong thing".

### 7.1 Failure classes

| Class | Detected by | Action |
|---|---|---|
| `locator_miss` | SDK browser/AX locator found nothing / ambiguous | **repairable**, auto-repair |
| `postcondition` | `expect(...)` failed, or output schema check | repairable, but **halts before writes** (postconditions checked before the first mutating effect where possible) |
| `data_drift` | parse error on input (column renamed, date format) | repairable |
| `auth_required` | sidecar sees login page / 401 | **user action**: "Log in to Jira in the Macroscope browser" (one click opens the managed profile) |
| `permission_denied` | broker denied (manifest) or OS TCC denied | user action; if manifest is the problem → repair *with permission diff review* |
| `app_unavailable` | target app not installed / not running / modal dialog blocking | retry with backoff, then user |
| `network` / `rate_limited` | transport errors, 429 | retry policy (3x, exponential), not a repair |
| `timeout` | executor | 1 retry, then classify from journal tail |
| `runtime_error` | uncaught exception | repairable |
| `llm_error` | runtime LLM call failed (`authRequired`, `quota`, per MSP TurnErrorKind / QuotaDetail [local, BRIEF]) | user action on key / wait until reset |

**Silent breakage** is the dangerous kind, so there are three extra detectors:

1. **Postconditions** in every generated app (the skill requires at least one per read
   step: non-empty, expected columns present).
2. **Pre-flight probes**: about 1 hour before a scheduled run, a cheap plan-mode run (read side
   only) catches drift in time to repair before the run is needed.
3. **"You did it by hand again"**: the pattern engine (track 09) sees the user performing
   an episode that matches an app's opportunity. The app card then asks "Did *Tuesday Jira
   status* not work for you?" This is the best signal we have that an app is broken or
   unused.

### 7.2 Self-repair loop

```
 FAILED(repairable)
     |
     v
 collect REPAIR PACKET -------------------------------------------------------+
   - manifest, current src/, failing run journal + log tail                   |
   - screenshot at failure point (managed browser) + last-good-run           |
     screenshot of the same step  (side-by-side)                              |
   - DOM / AX snapshot around the failed locator, recorded fallback locators |
   - redacted per track 08; screen text wrapped as DATA, not instructions    |
     |                                                                        |
     v                                                                        |
 MSP session/fork of the app's build session (keeps the original reasoning)  |
   on branch repair/<run_id>; `muse` with --image for screenshots             |
     |                                                                        |
     v                                                                        |
 agent patches src/ AND adds tests/agent/regression_<run_id>.test.ts          |
 built from the failing run's fresh fixtures                                  |
     |                                                                        |
     v                                                                        |
 executor gate (section 6.3 steps 1-5) + replay of the failing scenario     |
     |                                                                        |
     +--> red after 2 attempts --> BROKEN: notify user, keep app paused ------+
     |
     v
 CANDIDATE build N+1
     |
     +-- auto-promote IF: user opted in to auto-repair for this app AND
     |   diff touches only locators/parsing (no new effects, no permission
     |   change, no new irreversible step) AND app has no irreversible effects
     |
     '-- otherwise: "Fix ready" card with before/after screenshots + diff
```

Repair budget: max 2 automated attempts per incident, `--max-model-steps` capped (e.g. 40),
reasoning effort `medium`, and a monthly token budget per app, shown in settings. The numbers
are **[assumption]** until track 15 measures cost. Repairs are queued and run one at a
time, so a SaaS redesign that breaks 5 apps doesn't burn 5 parallel sessions. Repair the
most-used app first, then retry the rest against its learnings.

Prompt-injection note: a repair packet contains fresh screen content from a third-party
site, which is exactly the injection vector track 08 worries about. The repair agent runs
with the same sandboxed host posture as builds. Its output is constrained by the executor
gate, which matters most here: **a repair can never widen permissions or add irreversible
effects without a human approving a visible diff.** Content can push the agent toward bad
code, but it can't push the code past the manifest.

### 7.3 Making breakage rarer in the first place

- **Semantic locators**: role + accessible name + nearby label text. Avoid CSS paths.
- **Recorded fallbacks**: at build time the sidecar records, for each target element,
  `{role/name, text, testid, css, relative position to a stable landmark, a small image
  crop}`. At run time, if the primary locator misses but exactly one fallback matches with
  high confidence, the SDK **heals in place**: it uses the fallback, logs `healed`, and
  marks the run `succeeded_with_heal`. A background repair then updates the primary
  locator. Healing never applies to mutating actions unless the target's accessible name
  matches exactly, because clicking the wrong "Delete" is the worst outcome.
- **Prefer APIs over UIs**: if the Opportunity shows the web app has a public API, and the
  user agrees to create a token (stored in Keychain via `secrets`), the agent should use
  `http.*` over browser automation. It is more stable. The automatability score from track 09
  should drive this choice.

### 7.4 Versioning and rollback

- Every approved build = git commit + tag `vN` + signed tree digest. `current` points to one
  tag.
- **Rollback** flips `current` to an earlier tag. It is instant, and one click from the
  health view. Runs always record the build they used.
- The last 10 builds are kept. Older ones are squashed, but the tags of builds that
  produced journaled runs still within their undo window are kept.
- Builds that change `permissions` or add an irreversible effect **require re-approval**,
  no matter how they were produced (repair, user edit, compat sweep).
- The user can edit with natural language ("also include Done issues"). That is a new build
  session forked from the latest one, with the same gate.

### 7.5 App states and the circuit breaker

```
            approve                    3 consecutive failures
  DRAFT ----------> ACTIVE <-------------------------------+
                      |  ^                                 |
          fail (any)  |  | success / user fix / rollback   |
                      v  |                                 |
                   DEGRADED --(repairable)--> REPAIRING ---+--> ACTIVE (promoted)
                      |                          |
                      | 3rd consecutive fail     | 2 attempts red
                      v                          v
                   PAUSED (triggers off) <---- BROKEN (needs you)
                      |
                      +-- user: resume / rollback / archive
```

### 7.6 Health dashboard

```
 Automations                                  This month: 6.4 h saved (est.)  | Stop all
 -----------------------------------------------------------------------------------------
 *  Tuesday Jira status     ACTIVE    ..........x.:  19/20  last: today 14:00  next: Tue
 *  File Downloads invoices ACTIVE    ::::::::::::::  42/42  last: 11 min ago   watch
 !  Expense report filler   REPAIRING ......xx       6/8    "Save" button moved  [View fix]
 x  CRM weekly export       BROKEN    ...xxx          3/6    login required       [Log in]
 -  Calendar cleanup        PAUSED    paused by you                              [Resume]
```

Each app's detail view shows: run history (status, duration, build, trigger), the
latest plan, repair history with before/after screenshots, permissions, version list
with Rollback, time saved (the observed manual duration from the episodes × successful
runs, minus the time spent reviewing plan cards, shown as an *estimate*), and token
spend. The UX tracks (13/14) own the visual design. This section only specifies the
data model behind it: the `runs` table plus the app state machine.

---

## 8. Trust levels: when a run needs the user

| Level | Plan card shown | Irreversible effects | Reached by |
|---|---|---|---|
| `supervised` (default) | every run | each one confirmed inline | initial approval |
| `confirm-irreversible` | no (notification after) | **pauses at first irreversible effect** for a one-tap confirm (desktop notification; later the mobile companion) | offered after 3 clean supervised runs with zero edits |
| `unattended` | no | only if the user explicitly ticked each irreversible effect type (e.g. "send this email without asking") | explicit opt-in per app |

Any build that changes permissions or effects drops the app back to `supervised` for its
next run. Scheduled, file-watch and app-event triggers below the required level degrade
to "notify → plan card" and never run silently.

---

## 9. Browser automation: real profile vs managed profile

**Decision: v1 uses a managed profile. The user's real profile is reached in v2 through the
companion extension, not CDP.**

- Chrome 136+ ignores `--remote-debugging-port`/`--remote-debugging-pipe` on the
  default user data directory. A non-default `--user-data-dir` is required, and Google
  recommends Chrome for Testing for automation
  **[verified: https://developer.chrome.com/blog/remote-debugging-port]**. So "Playwright
  against the user's real Chrome profile" is effectively closed, and that's on purpose.
- Attaching to the user's real browser would also hand generated code every logged-in
  session they have. A managed profile contains **only the sites the user logged into
  for Macroscope**, scoped further per app by `permissions.browser.origins`.
- Cost: the user logs in once in the Macroscope browser window ("Connect Jira"). 2FA
  and "new device" emails happen once. Some SSO/device-trust setups (corporate
  Okta with device posture) will refuse it. That is a known limitation, and those apps fall
  back to v2.
- **v2**: the companion extension (track 06 uses it for capture) gets an "act" channel.
  The broker sends DOM actions to a content script in the user's real tab. This is
  powerful but needs the `userScripts`/`scripting` permissions, and it is in the user's live
  session, so it only runs at `supervised`/`confirm-irreversible` with an on-page overlay
  ("Macroscope is filling this form - Esc to stop").
- Headful vs headless: runs are **headful by default** in a small, unfocused Macroscope
  browser window during supervised runs, because seeing it work builds trust. Unattended runs
  are headless. Some sites behave differently headless, so the gate's live run uses the same
  mode as production.

---

## 10. Platform notes (macOS primary, Windows designed-for)

| Concern | macOS | Windows |
|---|---|---|
| Native app control | AppleScript/JXA via `osascript` spawned by the broker. The **Automation (Apple Events) TCC prompt** is attributed to Macroscope, appears once per target app, and onboarding should pre-explain it. | PowerShell 5.1 + COM (Outlook, Excel), UI Automation for apps without COM. |
| Shortcuts | `shortcuts run` (user-made Shortcuts as steps; we do not generate .shortcut files in v1, since signing/import is awkward) | n/a (Power Automate Desktop interop: later, maybe never) |
| Trash | `NSFileManager trashItemAtURL` | `IFileOperation` + recycle flag |
| Resource caps | watchdog + setrlimit (weak) | Job Objects (strong) |
| Sandbox for Python | `sandbox-exec` profile (deprecated but works; track 08) | restricted token + Job; AppContainer later |
| AV/EDR | low risk: signed, notarized helper spawning Deno | **higher risk**: PowerShell spawned by a background app that also watches input looks like malware to Defender/EDR. AMSI scans generated PowerShell. Mitigate: prefer COM through our signed recipes, sign our scripts, keep raw PowerShell rare. |
| Testing | owner tests on Mac | owner can't hand-test: CI on Windows runners runs every recipe's live test plus a replay corpus of sample apps. Windows recipes must be fewer and conservative in v1. |

---

## 11. Sharing and export (later)

**Phase 2: export for yourself.**
- *Export as standalone script*: the SDK has a fourth mode, `standalone`, which implements
  effects directly (no broker, no journal) for `fs`, `http`, `browser` (local Playwright) and
  `mac.*`. Export produces a folder with `main.ts`, the vendored SDK and a README with
  the exact `deno run --allow-...` line generated from the manifest. The permissions carry
  over as Deno flags. That's a nice property of picking Deno.
- *Export as binary*: `deno compile` with the same permission flags, giving a
  single-file executable **[assumption: compile embeds permission flags; verify]**.
  Unsigned, so it's for technical users only.
- **Fixtures, run logs and screenshots are never included.** Hardcoded personal values
  (emails, board URLs) are lifted into inputs by a "generalize" pass. That is a short Muse
  session with a locked test that runs the exported app with sample inputs.

**Phase 3: share with others (`.macroapp` bundle, maybe a gallery).**
- Bundle = repo tree minus fixtures/runs + manifest + author signature.
- Import is always `supervised`, shows the full permission review, and re-runs the gate
  in replay mode with the bundle's *synthetic* fixtures. Imported apps can't auto-promote
  for 7 days.
- A gallery should hold **templates** ("Weekly Jira → CSV → email draft"), not instances.
  The value is "Macroscope noticed you do X; 400 people automated X with this template;
  adapt it?" That also helps cold start for the pattern engine. Risks: a gallery is a malware
  distribution channel, and it needs review and moderation, which a solo dev can't staff.
  **I'd defer it until there are real users.**
- Team sharing (same app across a team's machines) belongs in a later business tier and is
  out of scope here.
- Mobile companion (later): approve `confirm-irreversible` steps, see failures, trigger
  manual runs remotely. It never runs anything itself.

---

## 12. Risks that could hurt the product

1. **A generated app does damage** (overwrites a file, emails the wrong people). Mitigations:
   brokered writes, Trash-not-delete, pre-images, drafts-not-sends by default, plan cards,
   trust levels. The remaining risk is irreversible SaaS actions in a browser. Those stay
   gated by confirmation in v1.
2. **The reliability treadmill**: if apps break weekly and repairs cost tokens and attention,
   users churn. Mitigations: API-first where possible, semantic locators + heal, pre-flight
   probes, budgeted repair. Measure this from the first week of dogfooding: *repairs per app
   per month* is a top-3 product metric.
3. **SDK/recipe maintenance burden** on a solo developer. Keep the SDK surface small, and
   prioritize recipes by what the owner's own captures show.
4. **Deno + Playwright compatibility, and V8 entitlements under notarization**. Both are
   unverified. Spike both in week 1, with bundled Node as the fallback for the sidecar.
5. **Bot detection / ToS**: automating some SaaS UIs violates their terms or trips
   anti-bot systems in the managed browser. Prefer official APIs, and be honest in the UI.
6. **Windows untested by the owner**: native-app automation on Windows is where silent
   failures would hide. Ship Windows with browser + file apps first, and native recipes only
   once CI covers them.
7. **Prompt injection through runtime LLM calls inside apps** (e.g. `summarize()` over Jira
   text): LLM output is data, and it can't add effects beyond the manifest. Apps whose LLM
   output flows into an irreversible effect (email body sent unattended) are capped at
   `confirm-irreversible`.

---

## Open questions for the owner

1. **Default for irreversible effects**: should v1 ever *send* (email/Slack/form submit)
   without a tap? I propose not in v1, with draft-only as the default. Is that too
   conservative for your own use?
2. **Auto-repair opt-in**: should selector-only repairs auto-apply by default (with a
   notification), or always wait for review? I lean toward default-on for apps with no
   irreversible effects.
3. **Per-app monthly token budget for repairs and runtime LLM calls**: what ceiling is OK
   on your Meta key (per app, and globally)?
4. **Python in v1 or v1.5?** It costs a lazy ~50 MB download and a weaker sandbox. Do your
   own workflows need pandas/openpyxl-class libraries early?
5. **Managed browser login friction**: are the web apps you'd automate first behind
   corporate SSO with device posture checks? If yes, the companion-extension "act"
   channel moves up to v1.
6. **Gallery ambition**: is sharing part of the business model (it changes how much
   generalization and signing infrastructure is worth building early), or purely a later
   nice-to-have?

## Decisions I made

- A micro-app is a git repo + `macroscope.yaml`, executed only by Macroscope's executor.
- The v1 primary artifact is TypeScript on the Macroscope SDK, with no custom YAML DSL.
- Deno (bundled, pinned) is the runtime; Bun is rejected (no permission model); Node is the fallback only.
- The micro-app process is pure compute: no net/run/env/ffi, writes only to scratch, and all effects are brokered.
- Python comes via bundled `uv` + lazily downloaded CPython, in v1.5, under an OS sandbox.
- Native apps go through our hand-written, tested recipes; raw AppleScript/PowerShell is an escape hatch that raises the risk tier.
- Browser automation uses a managed Chrome for Testing profile through a privileged Playwright sidecar, and never the user's default profile.
- Generated standalone browser extensions are out of v1; the companion extension "act" channel is v2.
- The SDK has plan / replay / live (+ standalone) modes, and "preview" is a plan-mode run.
- Deletes go to Trash, overwrites keep pre-images, and the journal-driven undo window is 7 days.
- Macroscope writes locked tests from the user's own episode; the agent can't edit them, and the executor verifies them itself.
- Nothing is offered until it passes the executor gate (typecheck, locked tests, golden plan, permission diff).
- Failures are classified, and only locator/data/runtime failures trigger the self-repair loop (max 2 attempts).
- A repair can never widen permissions or add irreversible effects without visible human approval.
- Each failing run becomes a regression test.
- A circuit breaker pauses an app after 3 consecutive failures; rollback is a pointer flip to an earlier signed tag.
- Trust levels are supervised → confirm-irreversible → unattended, and widening permissions resets to supervised.
- Macroscope's own scheduler runs schedules in v1, not launchd/Task Scheduler.
- "When I'm in app X" triggers only suggest and never auto-run.
- Export/standalone is phase 2, the gallery is phase 3, and fixtures are never exported.
