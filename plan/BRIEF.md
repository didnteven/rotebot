# Macroscope: planning brief (shared by every planning agent)

Today is 2026-10-22. Everything here is about planning; no app code exists yet.

## The idea (the owner's words, condensed)

A desktop app that watches what the user does on their computer (screen frames plus
interaction events), uses AI to find repetitive, tedious work, and then **builds small,
specific apps, scripts or automations** that do that work for them. Automation tools like
Zapier and Shortcuts make you decide what to automate and build it yourself; Macroscope
discovers it for you and writes the tool.

Original pipeline sketch (feel free to change any of it):

1. **Capture**: a background process records screen state only when the UI changes, plus
   events (active app, window titles, clicks, keystroke patterns but not content).
   Sensitive data is masked on the device before anything reaches a model.
2. **Pattern engine**: groups activity into recurring workflows ("every Tuesday: open Jira,
   copy three fields into a CSV, format dates, email a status update") and ranks them by
   time spent, frequency and cognitive load.
3. **Micro-app generator**: on "Automate this", an AI coding agent writes a dedicated
   tool (script, browser extension, menu-bar widget, Shortcut...), shows a preview or
   diff, the user tests it in a sandbox and pins it.

## Hard requirements from the owner

- **Desktop app for macOS and Windows. macOS is primary**: the owner can only test on a Mac.
  Windows must be designed for, but cannot be hand-tested by the owner.
- Mobile is out of scope for v1 (a prior analysis concluded mobile can only ever be a
  companion: approvals, notifications, triggers). At most, mention it as a later phase.
- **Must use Meta's APIs/models.** The owner specifically wants Meta's developer models,
  the **Muse** family, and floated using **Muse Code** (Meta's terminal coding agent)
  *from a terminal inside the application*, "or another way to build software/scripts
  inside of software".
- **All Meta model use is via API calls billed to an API key** (the owner's Meta developer
  API key), not a consumer account login. Every design that uses Muse (`muse serve`,
  `muse exec`) or calls Meta models directly MUST work with API-key auth, and the plan must
  say exactly how the key gets from our app to the model (see "API-key auth" below).
- **UI/UX matters**: plan the experience, not just the plumbing.
- The owner is a solo developer who builds with AI coding agents (Codex, Claude, Muse) and
  is comfortable with Flutter, Python, Rust and TypeScript.

## Verified local facts (checked on the owner's Mac today)

- `muse` CLI is installed: **Muse Code 1.4.11**. Full help text: `plan/reference/muse-cli-help.txt`.
  - `muse exec` runs one headless prompt (`--json` gives JSONL events, `--output-schema`,
    `--image`, `--workspace`, `--worktree`, `--approval-mode`, `--session-id`,
    `--max-model-steps`, `--reasoning-effort none..ultra`).
  - `muse serve` serves an **MSP (Muse Session Protocol) v1 session host over stdio**; the
    client owns stdin/stdout. Sandbox posture is fixed per host process (`--disable-sandbox`,
    `--sandbox-network`, `--disable-write`, `--disable-shell`); approval mode is chosen over
    the wire. Providers: `meta`, `local`, `echo`.
  - The full MSP v1 wire schema is exported at `plan/reference/msp/msp.d.ts` (TypeScript,
    ~3,600 lines) and `plan/reference/msp/msp.schema.json`. Methods include `session/start`,
    `session/resume`, `session/fork`, `session/read`, `session/sideChat`, `session/userShell`,
    `session/setModel`, `session/tokenUsage`, `session/contextUsage`, `approval/request`,
    `approval/decide`, `goal/set|pause|resume`, `subagent/*`, `task/*`, `skill/*`,
    `plugin/*`, `hook/list`, `media/upload`, `model/list`, `item/started|delta|completed`.
  - Known model ids: `muse-spark-1.3` (in the catalog) and **`muse-wonder-1.0`, which is
    the default model `muse exec` used in a test today** [local].
- **API-key auth (tested today)** [local]:
  - `META_API_KEY` env var: "always takes priority over the account login" (`muse login --help`).
    Verified: `META_API_KEY=<bogus> muse exec --json ...` sent the request to the Meta provider
    (model `muse-wonder-1.0`) and failed with
    `run.terminal.failed: "your API key from META_API_KEY was rejected — update or unset it"`.
    So the key path works for `muse exec`, and failures come back as structured JSONL.
  - `muse auth set --provider meta --api-key-stdin` stores a key in muse's own store (stdin only,
    never argv).
  - `muse exec --api-key-stdin` passes a key for a single run. `--base-url` overrides the endpoint.
  - `-c KEY=VALUE` on exec/serve accepts secrets only as whole-value `${VAR}` references.
  - MSP turn errors include `TurnErrorKind = "authRequired"` and a typed `quota` detail
    (`QuotaDetail`, reset instant), so a desktop client can show "key missing/rejected" and
    "rate-limited until X" without parsing text.
  - NOT yet verified: that `muse serve` picks up `META_API_KEY` from its environment
    the same way (expected, since it's the same binary and provider), and a *successful*
    call with a real key (the owner has to supply that).
- `codex` (0.163, has `codex app-server`) and `claude` (Claude Code) CLIs are also installed.
- The owner already runs `muse exec` headless overnight via a keeper script
  (`~/StudioProjects/agent-loop/scripts/muse_keeper.py`), so headless Muse is proven here.

## Claims from the earlier chat that are NOT verified

The idea came from a chat with another assistant. Treat these as **unverified, possibly
invented**, and verify with web search before relying on them: "Muse Wonder", "Muse Glimmer
(open-weight 30B)", "SAM 3.1 in the Meta Model API", base URLs like `dev.meta.ai/v1` or
`llama.developer.meta.ai/v1`, a `@meta/llama` npm SDK, Muse model context sizes, Llama 4
Scout's 10M-token context being usable for hours of screenshots. If you cannot verify
something, say so plainly and design so the plan does not depend on it.

## Rules for every planning agent

- Write only inside `plan/`. Your assigned output file is given in your task.
- Be concrete and opinionated: make decisions, give reasons, name the trade-off you accepted.
  Do not survey every option evenly.
- Mark every external fact as **[verified: source/URL]**, **[local: checked on machine]**
  or **[assumption]**.
- Call out risks that could kill the product (OS permissions, privacy, cost, App Store /
  notarization rules, prompt injection from screen contents, Meta API terms).
- End your file with **Open questions for the owner** (only real decisions) and
  **Decisions I made** (one line each).
