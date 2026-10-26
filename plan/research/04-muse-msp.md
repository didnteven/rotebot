# 04: Embedding Muse Code in Macroscope (MSP)

Track owner: planning agent 04 (Claude). Date: 2026-10-22. Muse Code 1.4.11 (`1.4.11-R6667.1`), MSP schema v1,
fingerprint `sha256:f282ad16…1082`.

Every claim is tagged **[observed]** (seen on the wire in a probe; the transcript is under
`plan/probes/transcripts/`), **[local]** (read from files or help on this Mac), **[schema]** (read in
`msp.d.ts` / `msp.schema.json`, not exercised) or **[assumption]**. The probe client is
`plan/probes/msp_probe.py`; `plan/probes/README.md` indexes the transcripts. Every probe used the free
`echo` provider or a **bogus** key. No paid call was made.

---

## 0. API-key auth: verified behaviour

### 0.1 What I ran and what came back

| # | Invocation | Muse home | Result (exact) | Transcript |
|---|---|---|---|---|
| A | `META_API_KEY=sk-invalid-test-key muse serve --provider meta --no-session-log`, then handshake, `session/start`, `turn/start "reply OK"` | real (`~/.local/share/muse`) | `initialize`, `model/list` and `session/start` all succeed (session `providerId:"meta"`, `modelId:"muse-wonder-1.0"`). `turn/start` is acked `accepted`. About 300 ms later the failure arrives **only** as the turn terminal: `turn/completed {terminal:"failed", error:{kind:"authRequired", message:"your API key from META_API_KEY was rejected — update or unset it", retryable:false}, reason:<same text>, durationMs:311}`. No JSON-RPC error, no stderr line, no retry notification, no `session/tokenUsage`. One `reminderChild` item settles `cancelled` ("main run failed"). | `02-meta-badkey-serve.jsonl` |
| B | Same bogus env key, but `XDG_DATA_HOME`/`XDG_CONFIG_HOME`/`XDG_STATE_HOME` point at an empty dir | isolated | At startup the host fetches the model catalog with the key, gets rejected and writes to **stderr**: `tbh serve: startup model-catalog fetch rejected the credential (… your API key from META_API_KEY was rejected …) — composing the logged-out fallback`, plus `the automated reviewer is unavailable; the default permission profile is :ask-me`. Then `model/list` returns `{source:"bundledCatalog", models:[]}` and the session comes back `providerId:"muse", modelId:null`. The turn fails `authRequired` with a **misleading** message: `"not logged in: run /login to add an API key"`. | `08-meta-badkey-isolated-serve.jsonl` |
| C | No key at all, isolated home | isolated | Same as B without the stderr line: `authRequired`, "not logged in: run /login…", turn `durationMs` 18–40 (no network). | `03-meta-nokey-isolated-serve.jsonl` |
| D | `printf sk-invalid… \| muse exec --json --api-key-stdin --no-session-log --max-model-steps 1 "reply OK"` | isolated | Exit 1. **Empty stdout (no JSONL)**. stderr: `failed to fetch model catalog: authentication failed: your API key from META_API_KEY was rejected — update or unset it`. Same with `--model muse-wonder-1.0`. | `04-…`, `06-…` |
| E | Same as D with `META_API_KEY` env instead of stdin | isolated | Identical to D. | `05-…` |
| F | Same as D (stdin key) | real | Exit 1. JSONL raw-log records ending in `run.terminal.failed` with reason "…META_API_KEY was rejected…"; stderr `run ended with Failed: …`. This matches what the BRIEF saw for the env var. | `07-…` |
| G | `muse exec --api-key-stdin --base-url http://127.0.0.1:18777` against a local fake endpoint that always returns 401 | isolated | muse sent `GET /muse-code/models` with `Authorization: Bearer <the stdin key>` and headers `x-client-id` and `user-agent: muse-build/1.4.11 (non-interactive; …)`. The 401 became the same "rejected" message. | `19-fake-endpoint-requests.jsonl`, `20-…` |

Conclusions:

1. **`muse serve` reads `META_API_KEY` from its process environment** [observed A, B]. The open question in the BRIEF is now closed.
2. **`--api-key-stdin` is the same credential lane as the env var.** The error text names `META_API_KEY` even when the key came in on stdin [observed D, F].
3. **The classification is the same everywhere** (`authRequired`, `retryable:false`), but **where it shows up depends on the cache state of the muse home**:
   - With no cached catalog (a fresh home, which is what Macroscope will have), `serve` quietly degrades to a "logged-out fallback". `model/list` comes back empty with `source:"bundledCatalog"`, and the turn error says "not logged in".
   - In the same state, `exec` dies before emitting any JSONL.
   - So a client must not rely on the message text. It should detect a bad key at host start from `model/list` (empty `bundledCatalog`) plus the stderr line, and at turn time from `error.kind == "authRequired"`.
4. Muse's own auth is `Authorization: Bearer <key>` on Meta's API [observed G]. A rejected key costs nothing: the catalog `GET` fails before any tokens are spent.

### 0.2 Successful call with a real key: NOT run

`META_API_KEY` is not set in this environment. Muse's stored credential is an **account login**: `mechanism:"oauth"`, `obtained_via:"device_code"`, `storage:"keychain"` [local; non-secret fields of `~/.config/muse/auth.json`]. The task forbids using it, so I did not. The successful-turn model id, token-usage events and latency are **unverified**.

What the schema says to expect [schema]:
- one `session/tokenUsage` per model completion, with `modelId`, raw `usage{inputTokens, outputTokens, cachedTokens, reasoningTokens}`, counted-once `promptTokens`/`totalTokens`, and `cumulative{…, cost?{usd, partial}}`;
- `turn/completed{usage, durationMs, timeToFirstTokenMs}`.

With echo I saw `session/tokenUsage` with zero counters and `modelId:null`, and `timeToFirstTokenMs` of 53 ms [observed 01]. The live catalog lists `muse-wonder-1.0` (default; released 2026-10-12; `contextLimit` 1,007,997; `outputLimit` 128,000; efforts minimal…max) plus `muse-spark-1.3` and `muse-spark-1.2`. **`cost` is `null` on every row** [observed 02], so Macroscope cannot depend on muse's `cumulative.cost` and needs its own price table.

**Next step for the owner:** run `python3 plan/probes/msp_probe.py --scenario goodkey --prompt "reply OK" --out plan/probes/transcripts/21-goodkey.jsonl --env META_API_KEY=$KEY -- --provider meta --no-session-log`, with the key passed only in the environment.

### 0.3 Incident: probe 13 broke the owner's `muse login`. Read before touching `account/*`

The experimental surface has `account/read`, `account/loginStart {type:"apiKey", apiKey}` and `account/logout` [local: `muse schema generate-ts --experimental`]. I ran them against a bogus key inside an isolated XDG home [observed 13]:
- `loginStart` answered `account/loginCompleted {outcome:"granted"}` **without validating the key**;
- `account/read` then reported `{state:"apiKey", label:"stored key"}`;
- the next `session/start` re-fetched the catalog and logged "your saved API key was rejected".

The problem: muse keeps the credential in **one login-Keychain item, service `ai.meta.dev.credentials`, account `meta`**, and XDG isolation does not scope that item. Its modification time is 2026-10-22 10:28:39Z, the second the probe ran [local: `security find-generic-password`]. After the probe, the owner's real muse reports `account/read → {state:"loggedOut"}` [observed]. **The owner must re-run `muse login`.** Until then, anything that uses the account login (such as the overnight keeper) will fail.

Design rule that follows: **Macroscope never calls `account/*`, `muse auth set`, `muse login` or `muse logout`.** Each of these writes, or probably writes, the shared credential. That would clobber the user's personal muse, and the user's personal muse would clobber Macroscope. `muse auth set` has not been tested; assume it shares the store.

### 0.4 Second finding: the key is visible to every shell command

`META_API_KEY` from the host environment is inherited by `session/userShell` commands. `env` printed `META_API_KEY`, `MUSE_SESSION_ID`, `MUSE_TOOL_USE_ID` and the `XDG_*` variables [observed 18]. Model shell tools almost certainly inherit it too [assumption: same spawn path]. So a prompt-injected build, for example from screen-derived content pasted into a spec, could `echo $META_API_KEY` and send it out if the network is open. `--sandbox-network restricted` blocked `curl` (`net=000`) where the default `proxy-only` allowed it (`net=200`) [observed 16].

### 0.5 Recommendation: storage, injection, rotation, one meter

```
 +-------------------- Macroscope shell (Rust) ---------------------+
 | KeyStore: Keychain item "app.macroscope.meta-api-key"            |
 |           (Windows: Credential Manager via `keyring`)            |
 |     |                       |                                    |
 |     | read at spawn         | read per request                   |
 |     v                       v                                    |
 | AgentBroker              PatternEngine HTTP client               |
 |  spawn muse-bin serve    Authorization: Bearer <key>             |
 |  env: META_API_KEY=key   User-Agent: Macroscope/x.y              |
 |       XDG_*=<our dirs>        |                                  |
 |       MUSE_NO_AUTO_UPDATE=1   |                                  |
 |       MUSE_LOGIN=0            |                                  |
 |     |                         |                                  |
 |     | MSP session/tokenUsage  | response.usage                   |
 |     v                         v                                  |
 |  UsageLedger (SQLite): {source, modelId, in, out, cached,        |
 |                         reasoning, $est, sessionId|callId, ts}   |
 +------------------------------------------------------------------+
```

- **Storage.** Macroscope owns its own Keychain item: generic password, service `app.macroscope.meta-api-key`, `kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly`, not synchronizable. On Windows, a Credential Manager generic credential through the `keyring` crate (DPAPI). Never in muse's store, never on disk in plaintext, never on argv.
- **Injection.** Only as `META_API_KEY` in the environment of the spawned `muse serve` child, built from an allow-list (`PATH`, `HOME`, `LANG`, `TMPDIR`, plus ours). The shell process keeps the key in memory only while spawning. Set `MUSE_LOGIN=0` and `MUSE_NO_AUTO_UPDATE=1`: the launcher honours both, so no device-login prompt and no self-update [local: launcher source]. Better still, spawn the versioned `muse-bin-<ver>` directly and skip the launcher.
- **Validation at entry.** When the user pastes a key, spawn a throwaway host and do `initialize` and then `model/list`.
  - `source:"providerCatalog"` with models present means the key is good.
  - `source:"bundledCatalog"` with `models:[]`, plus the stderr line "fetch rejected the credential", means it is rejected.
  - Zero tokens either way [observed B vs A].
  - The pattern engine's own `GET` on the Meta models endpoint is an equivalent check once track 03 confirms the public base URL. The path muse uses is `/muse-code/models` [observed G]; the public base URL is unknown.
- **Rotation (planned).** The env is fixed for the host's lifetime. Rotate by: (1) mark hosts *stale*; (2) let in-flight turns finish (no new `turn/start`); (3) close stdin, after which the host emits `session/closed{reason:"hostShutdown"}` and exits 0 [observed]; (4) respawn with the new key; (5) `session/resume` each open build session. Resume after a host restart works and history is kept [observed 11]. Users see a 1–2 s "Reconnecting builder…" chip.
- **Revocation or rejection (unplanned).** The first `turn/completed` with `error.kind=="authRequired"` puts the broker into *KeyRejected*:
  - pause all queued work;
  - send `goal/pause` to any session in goal mode;
  - show a blocking banner, "Your Meta API key was rejected. Replace key";
  - after the replacement, follow the rotation path and offer "Retry last step" (a re-send of the last user input).
  - Do not retry automatically (`retryable:false`).
- **One key, one meter.** The pattern engine calls the Meta API directly with the same Keychain key. Both paths write to the same UsageLedger:
  - muse usage comes from `session/tokenUsage` (per completion, with `modelId`);
  - subagent usage comes from `subagent` items' `usage`, which is "never folded into cumulative" [schema], so it must be added separately;
  - `reminderChild` sessions are their own child sessions, and their usage may not be in the parent stream [assumption]. Reconcile against the Meta console monthly and flag drift over 5%.
  - Cost comes from Macroscope's price table, because catalog `cost` is null.
  - The user agent tells the provider which traffic is which: `muse-build/1.4.11 (serve; …)` [observed] versus `Macroscope/x.y`.
- **Exfiltration mitigations, in order:**
  1. Build hosts default to `--sandbox-network restricted` (§2.2).
  2. Builds whose prompt carries screen-derived text always run restricted.
  3. Ask Meta whether keys can be scoped or spend-capped. If they can, recommend a dedicated "Macroscope builder" key.
  4. Later, a local key-injecting proxy: muse would get a per-host dummy token and the proxy would swap in the real key. This works today for `exec` (`--base-url` is honoured [observed G]) but **not for `serve`**: `--base-url` is rejected and no `-c` key for it exists [observed 19, 14a].

---

## 1. The wire protocol as observed

| Fact | Status |
|---|---|
| Framing is **newline-delimited JSON-RPC 2.0** over stdio (one JSON object per `\n`). No `Content-Length` headers. | observed 01 |
| Handshake: `initialize{clientInfo, capabilities}` returns `InitializeResult`. The client then sends the `initialized` notification. Requests sent before `initialize` get `-32600 notInitialized`; a second `initialize` gets `-32600 alreadyInitialized`. | observed 15 |
| A bad JSON line gets `{"id":null,"error":{"code":-32700,"kind":"parseError"}}` and the connection **survives**. | observed 15 |
| `InitializeResult` carries `serverInfo.version:"1.4.11"`, `schema{version:1, fingerprint}`, `museHome`, `platformOs:"macos"`, `sessionDurability` (`ephemeral` with `--no-session-log`, `durable` otherwise), `sandboxAvailability:"available"`, and `userAgent`. | observed |
| Requested `userShell`, `sessionListStream`, `feedback` and `sessionRename` were all granted on durable hosts. `sessionRename` is withheld on ephemeral hosts. | observed 01, 10 |
| Cold `initialize` round trip is 0.5–1.0 s (first spawn) and about 0.1 s warm. `session/start` takes 0.35–0.5 s. An echo turn takes 0.1 s (ephemeral) to 0.5 s (durable). | observed |
| Every server→client frame carries `emittedAtMs`. View events carry an opaque `viewCursor` (`v4:<session>:<n>`, deltas `n.1`, `n.2`…). | observed |
| Closing stdin is an orderly shutdown: `session/closed{reason:"hostShutdown"}`, then `statusChanged notLoaded`, then exit 0. | observed |
| An unknown `-c` key or argument makes the host **refuse to start**. Only stderr says why, and `initialize` never gets an answer. | observed 14a, 19 |

Observed turn sequence (echo, durable host):

```
-> turn/start {commandId:C, sessionId, input:[{type:text,text}]}
<- result {turnId:C, disposition:"started", startedNewTurn:true}      (turnId == commandId)
<- session/statusChanged {status:"running"}
<- turn/started {turnId:C}
<- item/completed  kind=userMessage
<- item/started    kind=reminderChild           (background "goal-reminder")
<- item/started    kind=agentMessage text=""
<- item/delta      field=text delta="echo: …"   (no sourceRange; never bumps revision)
<- session/tokenUsage / session/contextUsage {windowTokens:1007997, usedTokens, pressure:"normal"}
<- item/completed  kind=agentMessage rev=2 (full text)
<- turn/foregroundCompleted {blockingAgents:["goal-reminder","verify-reminder"]}
<- item/completed  kind=reminderChild …         (reminders run AFTER the answer)
<- session/statusChanged {status:"idle"}
<- turn/completed {terminal:"completed", durationMs, timeToFirstTokenMs}
```

Two traps showed up:
- **Reminder agents** (`goal-reminder`, `verify-reminder`) run after every turn as child sessions. With echo they failed ("provider does not support base instructions"). With `meta` they are probably extra billed model calls [assumption]. `muse exec` has `--disable-reminders`; `serve` has no such flag, and the matching `-c` key is unknown.
- **`turn/completed` must be matched by `turnId`.** After a crash, `session/resume` emits a stale `turn/completed` for the orphaned turn. My probe briefly mistook it for the new turn's terminal [observed 14].

Approvals: **not observed.** The echo provider never calls tools, and a real model needs a key. Everything in §3.3 comes from the schema.

---

## 2. Hosting model

### 2.1 Decision: one long-lived host per *posture*, shared by all micro-apps

```
Macroscope shell (Rust)
 ├─ AgentBroker
 │   ├─ host "build"   : muse-bin serve --provider meta --sandbox-network restricted
 │   │                   (durable sessions; serves every micro-app's build sessions)
 │   ├─ host "build+net": … --sandbox-network proxy-only     (spawned on demand, idle-killed after 10 min)
 │   └─ host "plan"    : … --disable-write --disable-shell   (spec drafting / "explain this", optional)
 └─ PTY manager (advanced terminal, §4)
```

Reasons:
- MSP is multi-session per host: sessions are cheap, and approvals and events are tagged with `sessionId`.
- The only things that must be per-process are sandbox posture, durability and the env, which includes the key [local: `serve --help`].
- A host per micro-app would multiply cold starts (about 1 s plus a catalog fetch), reminder agents, and processes that map a 387 MB binary, for no isolation gain. Every session already gets its own `workspaceRoot`.

Trade-off accepted: a crash takes down all builds on that host at once. The mitigation is cheap: respawn, then `session/resume` [observed 14].

Exception: a micro-app the user marks as "sensitive" (one whose fixtures contain captured data) always builds on a restricted host, even if a network host exists.

### 2.2 Posture per host

| Host | Flags | Use | Why |
|---|---|---|---|
| build (default) | `--sandbox-network restricted`, write on, shell on, **no** `--trust-workspace` | All generated micro-apps | The key is in the env (§0.4), and build context can carry screen-derived text, so no egress by default. `restricted` blocked curl [observed 16]. |
| build+net | `--sandbox-network proxy-only` (default proxy egress) | Only after the user approves "This build needs to download packages (pip/npm)" on a card | Proxy egress worked [observed 16]. The card names the reason. The session moves to this host with `session/leave`, then `session/resume` on the other host [schema]. |
| plan | `--disable-write --disable-shell` | "Draft the automation spec", "explain this workflow" | `--disable-shell` made userShell fail with "tool policy denied shell execution" [observed 16]. Cheap and safe. |

Never `--disable-sandbox` or `--yolo`. If `initialize` reports `sandboxAvailability:"unavailable"`, refuse to start build hosts and show a diagnostic. Never fall back silently.

Approval mode is per session over the wire: use `approvalMode:"promptUnmatched"` on `session/start`. It was accepted and echoed as `session/approvalModeChanged` [observed 09]. The host's LLM approval judge is on by default (`--approval-judge`). Keep it on; render its decisions (`resolvedBy:"llmJudge"`) visibly.

### 2.3 Durability and isolation of muse's state

- **Use durable sessions; never `--no-session-log` for build hosts.** On an ephemeral host, `session/fork` and `session/read` return `-32601 methodNotFound` [observed 09], and resume after a crash is impossible.
- **Give muse its own home.** Point `XDG_DATA_HOME`, `XDG_CONFIG_HOME` and `XDG_STATE_HOME` at `~/Library/Application Support/Macroscope/muse/{data,config,state}` (`%APPDATA%\Macroscope\muse\…` on Windows [assumption: XDG is honoured there]).
  - Verified effects: `museHome` moves, and `session/list` stops showing the user's personal sessions (with the real home it listed the owner's `~/StudioProjects/confidence` sessions [observed 09]).
  - User skills and plugins stop leaking in.
  - Exception: "foreign personal" skills from `~/.claude/skills` still appear unless `HOME` is overridden too [observed]. Overriding `HOME` breaks git identity and toolchains, so v1 accepts this leak and lists it in §6.
  - XDG isolation does **not** isolate the Keychain credential (§0.3).

### 2.4 Workspace layout per micro-app (aligned with track 10)

```
~/Library/Application Support/Macroscope/
  apps/
    app_01JB…/                      <- one git repo per micro-app (session workspaceRoot)
      macroscope.yaml               <- manifest (track 10)
      src/  tests/  fixtures/
      .git/
    app_01JB…-attempts/
      b/                            <- `git worktree add` for "try another approach"
  muse/{data,config,state}/         <- isolated muse home (sessions live in data/muse/sessions)
  muse/config/muse/skills/macroscope-microapp/SKILL.md   <- our skill pack (user scope)
```

- One repo per tool. Macroscope **commits after every completed turn**, with the `turnId` in the commit trailer (`Muse-Turn: <id>`). That gives rewind, diff review and "discard partial changes" after a crash, all without relying on muse checkpoints.
- No `--trust-workspace`. The repo is written by the model, so a prompt-injected `AGENTS.md` or `.agents/skills/` would otherwise be loaded as rules on the next session. Project skills are skipped when untrusted ("project-skills-untrusted") [observed]. Macroscope's conventions come from the user-scope skill pack instead (§3.6).

---

## 3. MSP to Macroscope UI mapping

```
+-- Micro-app "Jira → CSV status" ------------------------------------------------+
| Build ▾ (session 3 of 3)   [Rewind]  [Try another approach]  [Terminal ⌘`]      |
|---------------------------------------------------------------------------------|
| you:  Build the automation from spec v2                                          |
| muse: I'll create macroscope.yaml and a Deno script…        (agentMessage, live) |
|  ▸ Thinking (reasoning summary)                                                  |
|  ▸ Edited 3 files  +84 −2  [View diff]               (toolCall + patchSummary)   |
|  ┌ Approval needed ─────────────────────────────────────────────────────────┐   |
|  │ Run:  deno test --allow-read=fixtures                                     │   |
|  │ in    apps/app_01JB…                                                      │   |
|  │ [Allow once]  [Allow for this build]  [Deny ▸ tell Muse why…]             │   |
|  └───────────────────────────────────────────────────────────────────────────┘   |
|  ◌ Checking work… (verify-reminder)                                              |
|---------------------------------------------------------------------------------|
| Goal: "CSV matches fixture; email draft created"  ███████░░░ 70%  [Pause]        |
| Context 41% · This build ≈ $0.38 · ⏵ 1 background task                           |
+---------------------------------------------------------------------------------+
```

### 3.1 Concept table

| MSP concept | Macroscope UI | Notes |
|---|---|---|
| `session/start` (`workspaceRoot` = app repo) | A **Build session** of a micro-app. Version 1 is one session; "Improve" and "Repair" (track 10) start new sessions in the same repo. | Name it with `session/rename` ("Build 1: CSV cleanup") [observed]. Keep the sessionId in our DB next to the app. |
| `turn/start` | The composer. The first turn is generated from the automation spec. | `ifBusy:"queue"` by default. Queued turns show as "Queued" chips; `turn/unqueue` gives Undo. |
| `turn/interrupt {retract:true}` | Stop button. Restores the prompt text if nothing was produced yet. | [schema] |
| `turn/steer` | "Add a note while it works". | [schema] |
| `item/started/delta/completed` | Streaming transcript. Deltas append to `text`, `summary.N` or `output`. `item/completed` replaces the item when its `revision` is higher. | Unknown `kind`: render `fallbackText` and status generically [schema rule]. |
| `toolCall` + `patchSummary` / `patchRef` | "Edited N files" row, with a diff viewer fetched through `item/readOutput(patchRef.id)`. | [schema] |
| `reasoning` | Collapsed "Thinking" disclosure. | |
| `reminderChild` | A single "Checking work…" chip, hidden by default. | Observed after every turn. |
| `turn/foregroundCompleted` | Answer done, verification still running. Enable the composer and show the chip. | Observed. The turn ends only at `turn/completed`. |
| `turn/completed` failed | Error row typed by `error.kind`. `quota` gives a countdown. | §5 |
| `turn/retryScheduled`, `turn/modelRetryScheduled` | "Retrying in 8 s (2/5)". | [schema] |
| `session/statusChanged` + `attention:["approvalPending"]` | Sidebar badge; macOS notification when the window is in the background. | Observed (no attention flag seen). |
| `approval/request` | Approval card (§3.3). | [schema] |
| `userInput/request` | Inline multiple-choice question card. | [schema] |
| `goal/set/edit/pause/resume/clear` + `session/goalChanged` | The **automation spec's "Done when…" line** becomes the goal objective. Progress bar from `percentComplete`, with `currentWork`/`nextWork` as subtitles. | §3.4 |
| `session/todoListChanged` | Checklist in the build sidebar. | [schema] |
| `subagent` items, `subagent/*` | Nested "Helper: write tests" rows with a drill-down (`childSessionId` → `session/read`) and Stop (`subagent/stop`). | Children's approvals arrive with `subagentOrigin` on the parent [schema]. |
| `toolCall.background`, `task/list`, `task/stop`, `task/stopAll` | "⏵ 1 background task" pill (for example a watcher or long test); Stop. | `task/list` returned `[]` [observed]. |
| `session/fork` | **Rewind**: continue from the conversation at the end of this build. | §3.5 |
| new `session/start` in a git worktree | **Try another approach**. | §3.5 |
| `session/userShell` | The "Run" bar: one-shot commands like `deno test`, output in the transcript. | §4 |
| `session/sideChat` | "Ask about this build", a question that does not pollute build context. | [schema] |
| `session/tokenUsage`, `session/contextUsage`, `session/compact` | Footer: tokens, $ (our price table), context % with a "Compact" action at `warning` pressure. | contextUsage observed. |
| `model/list`, `session/setModel`, `session/setReasoningEffort` | Settings, then "Builder model" (default `muse-wonder-1.0`) and effort. | Catalog observed. |
| `skill/list`, `plugin/*` | Hidden from users. Macroscope installs its own pack (§3.6). | Observed lists. |
| `view/gap`, `view/page` | Invisible: the client repairs the transcript with `view/page`. | [schema] |

### 3.2 Event handling rules for the client

- Take `turnId` from the `turn/start` ack, never from the `commandId`. They are equal today [observed], but the schema calls the ack authoritative.
- Match terminals by `turnId`.
- Every command needs a client-minted UUIDv7 `commandId`. A resent command with the same id is idempotent [schema]. That is how the broker retries safely after a respawn.
- `session/userShell` right after a turn can fail with `-32030 commandRejected reason:"runtime_busy" retryable:true` while reminders run [observed 09]. Retry with backoff of 0.5 s, 1 s, 2 s, and so on.
- `goal/pause` on an already-paused goal fails with `invalid_goal_state` [observed]. Read the goal state from `session/goalChanged` before acting on it.
- Treat every enum as open. Ignore unknown notifications and render unknown item kinds generically.

### 3.3 Approval cards (schema-derived; not exercised)

```
server -> client  {"id":7,"method":"approval/request","params":{approvalId, currentRequirementId,
                   availableChoices:[{choiceId,label,decision,scope,acceptsFeedback,rulePreview}],
                   subject:{kind:"shell"|"fileAccess"|"network"|…, command|path|host|stages[]},
                   toolName, itemId, turnId, judgeEscalated, protectedWrite, subagentOrigin?}}
client -> server  {"id":7,"result":{}}                      <- presentation receipt, immediately
user clicks
client -> server  approval/decide {sessionId, approvalId, choiceId,
                   requirementId: <currentRequirementId>, commandId, feedback?}
server -> client  result {terminal:true|false}               <- false: another stage follows
server -> client  approval/updated (next stage) …  approval/resolved {decision, resolvedBy}
```

- Card body by `subject.kind`: shell shows the argv per stage, fileAccess shows path and access, network shows host:port. Unknown kinds get a generic card that is **never auto-approved**.
- Show `protectedWrite` with a warning style.
- Buttons come straight from `availableChoices`. Hide `scope:"localPersistent"` in v1, so no rules persist into muse's home where Macroscope cannot see them. `acceptsFeedback` choices open a "Tell Muse why" field.
- Late join and reconnect: after `session/resume`, pending requests are re-issued, and `approval/listPending` gives the list [schema]. It returned empty lists [observed 11].
- The race guard: if the decision fails with `approvalRequirementStale` or `approvalAlreadyResolved`, refresh the card from `approval/updated` or `approval/resolved`. Do not show an error.
- `approval/reviewing` (judge working) shows "Reviewer checking…". `resolvedBy:"llmJudge"` shows as a small "auto-approved by reviewer" note, with a setting to require human approval for everything.

### 3.4 `goal/*` = the automation spec's "done" condition

Observed [09, 10]:
- `goal/set` with an idle session **immediately launches a goal turn**, followed by an automatic turn: "Continue working toward the active session goal. The objective below is user-provided data…".
- With echo, the goal then auto-paused (`status:"paused"`).

So goal mode is an autonomous loop that spends tokens without user input. Macroscope uses it as an explicit **"Build until done"** button. The objective is the spec's acceptance line, such as "`deno test` passes and output matches fixtures/expected.csv". Guards:
- a per-build budget (default $2): the broker sends `goal/pause` when cumulative ledger cost crosses it;
- a wall-clock cap;
- pausing whenever an approval is pending for more than 10 minutes.

### 3.5 Rewind vs. "Try another approach"

Observed:
- `session/fork` **without** `cutPoint` works. It creates a new durable session with the full conversation, and the parent survives [12].
- `cutPoint:{lastTurnId}` failed with echo: `forkBoundaryInvalid reason:"invalid_cut" seedFallbackReason:"no_usable_checkpoint"` [11]. After a `goal/set` it failed with `historical_goal_cut_unsupported` [10]. Mid-history cuts are **unreliable**; whether `meta` sessions have usable checkpoints is unverified.
- **The fork shares the parent's `workspaceRoot`**. `turn/start.workspaceRoots:[<worktree>]` was accepted, but `userShell` still ran in the original directory [observed 17]. A fork is a conversation branch, not a file branch.

Design:
- **Rewind to here** (on any earlier turn): `git reset --hard <commit for that turn>` in the app repo (we commit per turn), then `session/fork` (no cut), then a first turn in the fork that says "Files were reset to after step N; disregard later steps." The conversational context is approximate; the files are exact.
- **Try another approach**: `git worktree add ../attempts/b`, then a **new** `session/start{workspaceRoot: worktree}`. The first turn is generated by Macroscope: spec, a summary of what attempt A did (from our ledger), and the reason the user gave for trying again. Attempts sit side by side in a compare view. "Keep this one" merges or fast-forwards the worktree branch.

### 3.6 Teaching Muse to write Macroscope micro-apps

- Ship a skill pack, `macroscope-microapp` (the manifest schema, SDK API, test conventions, packaging rules), and install it **at user scope in the isolated muse home** with `muse skills install <dir> --scope user` under our XDG env. Verified: it lands in `<XDG_CONFIG_HOME>/muse/skills/` and lists as `scope:user`, with no workspace trust needed [observed].
- Project skills are discovered at `.agents/skills/<id>/SKILL.md` (and `.claude/skills/`, but **not** `.muse/skills`) and only under `--trust-workspace` [observed]. We avoid trust, so we don't use them.
- In the first build turn, invoke the skill explicitly with an input part `{type:"skill", selector:"macroscope-microapp"}` before the text part [schema: `TurnInputPart.type` includes `skill`; combining parts is assumed to work].
- In v1.1, move to a **plugin bundle** installed with `plugin/install {target:{localPath:{path, scope:"user"}}}`. It would carry skills plus a `postToolUse` hook that runs `macroscope validate` (manifest lint) after edits, surfaced as `hookRun` items [schema].
- Pin the skill pack version to the Macroscope release. Re-install on upgrade (`skill/changed` tells open sessions).

---

## 4. Raw embedded terminal: yes, as an "advanced" surface

Offer two PTY tabs in a bottom drawer (xterm.js in the webview, `portable-pty` in Rust; SwiftTerm is macOS-only and the UI is shared with Windows):

1. **Shell**: the user's login shell in the app repo, for running and debugging the micro-app. `META_API_KEY` is **not** injected; it is not needed to run apps.
2. **Muse TUI**: the same pinned `muse-bin` with the same env (key, XDG, `MUSE_LOGIN=0`), running `muse resume <sessionId>` in the app repo, for power users who want the native TUI (slash commands, `!` shell, voice).

How the TUI and the structured UI coexist on one session (a single-writer rule):

```
structured UI owns session S        user clicks "Open in Muse TUI"
  └─ broker: session/leave {S, stop:false}  ──► session/leaveCompleted {released}
  └─ PTY: muse-bin resume S                     (TUI now holds S)
  └─ UI shows S read-only: "Open in terminal"; periodic session/read for the transcript
TUI exits (PTY closes)
  └─ broker: session/resume S  ──► history + pendingRequests; UI live again
```

Status: `session/leave` and `sessionInUse` (-32021) are [schema]. The TUI's lease behaviour is an [assumption] to test in the first spike.

Never run both writers at once. Approvals raised while the TUI owns the session appear only in the TUI. The UI shows "Approvals are in the terminal".

The "Run" bar (`session/userShell`) is *not* a terminal. It is a non-interactive command whose output becomes a transcript item, which the model can see [schema SS3.9]. It covers 90% of what non-power users need.

---

## 5. Distribution, versioning and failure modes

### 5.1 Bundle or require install? Require install (v1)

What is on disk [local]:
- `~/.local/bin/muse` is a **bash launcher** (launcher v3) that downloads `muse-bin-<ver>` from `lookaside.facebook.com` on channel `muse-stable` or `muse-canary`. It checks for updates **hourly in the background** (`MUSE_UPDATE_INTERVAL_SECONDS`), and may run a Meta **account device login** when the download endpoint answers 401/403/404 (`MUSE_LOGIN`).
- The launcher only knows `aarch64/x86_64 macOS` and Linux. **There is no Windows path in it**, although the schema has `platformOs:"windows"` and the CLI has `muse sandbox windows setup`.
- The binary is a 387 MB Mach-O arm64, Developer-ID signed by Meta (Team `V9WTTPBFK9`) with the hardened runtime.
- The update notice records `1.4.6 → 1.4.11`, and 1.4.6 is gone, so **old binaries are deleted on update**.

Decision:
- Do **not** bundle. The redistribution terms are unknown; we would carry 387 MB per architecture; and Meta ships fixes hourly.
- Onboarding detects muse in this order: `$MACROSCOPE_MUSE_BIN`, then `~/.local/bin/muse` resolved through `.muse-version` to `muse-bin-<ver>`, then `which muse`. If absent, it links to Meta's official install instructions [assumption: a public installer exists; the launcher self-references `https://api.meta.ai/muse-launcher.sh`].
- The broker **spawns `muse-bin-<ver>` directly** (no launcher, so no surprise update mid-build). It re-resolves the path only between builds.
- Windows: blocked until Meta publishes a Windows install path. Plan the broker so the binary path and the `muse sandbox windows check|setup` step are pluggable. This is an open question.

### 5.2 Version pinning against the schema fingerprint

- Macroscope ships `known_good.json`: `[{version:"1.4.11-R6667.1", fingerprint:"sha256:f282ad16…1082"}]`, generated in CI from `muse schema generate-json-schema` and checked against `plan/reference/msp/manifest.json`.
- At each host start, compare `initialize.schema.fingerprint` and `serverInfo.version`:
  - **Exact match**: go.
  - **Fingerprint differs, `schema.version` is still 1**: the schema says this is "a warning condition, not an error". Run the **echo conformance smoke test**: spawn `serve --provider echo` in a temp XDG home and replay our golden script (handshake, start, turn, userShell, fork, resume, error frames). Each field we depend on must be present. On pass, record the new pair as provisionally good and show nothing. On fail, disable building, show "Muse Code updated to X; Macroscope needs an update", and keep everything else in the app working.
  - **`schema.version != 1`**: hard stop.
- The client is generated from the TypeScript/JSON bundle of the *pinned* version, decodes tolerantly (open enums, unknown fields ignored), and never sends `experimentalApi:true` in production.

### 5.3 Failure modes

| Failure | How it shows up | Macroscope response |
|---|---|---|
| muse not installed | spawn `ENOENT` | Onboarding step "Install Muse Code" with a re-check button. The rest of Macroscope (capture, patterns) still works; building is disabled. |
| Wrong version / drift | fingerprint mismatch in `initialize` | §5.2 smoke test; degrade only the build feature. |
| Host refuses to start (bad flag or `-c`) | process exits; stderr reason; no `initialize` reply [observed] | 10 s handshake timeout; attach the stderr tail to the diagnostic; this is our bug, so report it in telemetry. |
| Key missing | `turn/completed` failed, `authRequired`, "not logged in" [observed]; at start, empty `bundledCatalog` | Key setup sheet; never suggest `muse login`. |
| Key rejected or revoked | the same `authRequired`; stderr "fetch rejected the credential" at start [observed] | KeyRejected state (§0.5). |
| Rate limit / quota | `turn/completed.error.quota{limitKind, resetsAtMs}`; `turn/retryScheduled` / `modelRetryScheduled` with `httpStatus`, `retryDelayMs` [schema] | Countdown chip; with goal mode, `goal/pause` until reset; queue new builds. |
| Overloaded / backpressured | `-32001` / `-32031`, `retryable:true` [schema] | Exponential backoff with the same `commandId`. |
| Host crash mid-turn | stdout EOF / non-zero exit (SIGKILL gave -9 [observed]) | Respawn and `session/resume`. The orphaned turn arrives as `turn/completed{terminal:"cancelled", reason:"resume_reconcile:orphaned_by_process_loss"}`, also present as `lastTurn` [observed 14]. Then `git status`: if the work tree is dirty compared with the last per-turn commit, offer "Keep partial changes" or "Discard". |
| Macroscope crashes | host sees stdin EOF and shuts down cleanly [observed] | On relaunch, resume sessions that were running and show "Interrupted" on them. |
| `runtime_busy` | `-32030` `retryable:true` [observed] | Backoff retry. |
| Event loss | `view/gap{after,next}` [schema] | Re-page `(after, next)` with `view/page` and splice. |
| Sandbox unavailable | `initialize.sandboxAvailability:"unavailable"` [schema] | Block building; explain why. |
| Session held by the TUI | `-32021 sessionInUse` [schema] | Show "Open in terminal". |
| Shared-credential clobber | muse reports `loggedOut` [observed §0.3] | Prevented by never using `account/*` or `auth`. |

---

## 6. Risks that could hurt the product

1. **Key exfiltration through the agent's environment** (§0.4). This is the biggest one. Mitigated by restricted network and no screen-derived text on network hosts. A real fix needs `serve` to accept a base URL so a local proxy can hold the key, or Meta-side scoped keys. Raise this with Meta.
2. **Silent or misleading auth failures.** "Not logged in" when the key was actually rejected [observed B]. Mitigated by our own key validation and by the stderr parse at start.
3. **Hidden spend.** Reminder agents after every turn, goal-mode autonomous turns, and subagent usage outside `cumulative`. Needs the ledger, budgets, and one real-key measurement of reminder cost (the owner's next step).
4. **Upstream churn.** Hourly updates that delete old binaries mean a user can wake up on a new fingerprint. The smoke test catches it, but a breaking change disables building until we ship. Keep the MSP client thin and generated.
5. **Windows.** No verified install path. The Windows build feature may lag the Mac.
6. **Personal-context bleed.** `~/.claude/skills` leaks into sessions even under XDG isolation [observed]. That is mostly harmless, but it makes builds machine-dependent. Revisit overriding `HOME` if it causes drift.
7. **Terms.** Embedding a Meta CLI in a third-party app and driving it with the user's key is presumably fine for a personal tool. Distribution to others needs Meta's developer terms checked [assumption, unverified].

---

## 7. Minimal client sketch (Rust, tokio)

This matches what was observed: NDJSON framing, the handshake, a bad-key check at start, one turn, approvals, and the turnId-matched terminal. The Rust shell owns this; the UI receives the events over Tauri IPC.

```rust
// deps: tokio (full), serde_json, uuid (features = ["v7"]), anyhow
use serde_json::{json, Value};
use std::{collections::HashMap, path::Path, process::Stdio, sync::Arc};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{ChildStdin, Command};
use tokio::sync::{mpsc, oneshot, Mutex};

const KNOWN_GOOD_FP: &str = "sha256:f282ad16e21520d0cda68cb190149b86e2b062eef1dd282749360c05aa3d1082";
fn cmd_id() -> String { uuid::Uuid::now_v7().to_string() }

pub struct Msp {
    stdin: Mutex<ChildStdin>,
    next_id: Mutex<u64>,
    pending: Arc<Mutex<HashMap<u64, oneshot::Sender<Value>>>>,
}

impl Msp {
    pub async fn spawn(muse_bin: &Path, key: &str, home: &Path)
        -> anyhow::Result<(Arc<Msp>, mpsc::UnboundedReceiver<Value>)> {
        let mut child = Command::new(muse_bin)
            .args(["serve", "--provider", "meta", "--sandbox-network", "restricted"])
            .env_clear()
            .env("PATH", std::env::var("PATH")?).env("HOME", std::env::var("HOME")?)
            .env("META_API_KEY", key)                      // the ONLY way the key enters muse
            .env("XDG_DATA_HOME", home.join("data"))
            .env("XDG_CONFIG_HOME", home.join("config"))
            .env("XDG_STATE_HOME", home.join("state"))
            .env("MUSE_LOGIN", "0").env("MUSE_NO_AUTO_UPDATE", "1")
            .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped())
            .kill_on_drop(true).spawn()?;
        let (tx, rx) = mpsc::unbounded_channel();
        let pending: Arc<Mutex<HashMap<u64, oneshot::Sender<Value>>>> = Default::default();
        let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
        let p = pending.clone();
        tokio::spawn(async move {
            while let Ok(Some(line)) = lines.next_line().await {          // NDJSON: one frame per line
                let Ok(msg) = serde_json::from_str::<Value>(&line) else { continue };
                let is_response = msg.get("method").is_none()
                    && (msg.get("result").is_some() || msg.get("error").is_some());
                if let (true, Some(id)) = (is_response, msg["id"].as_u64()) {
                    if let Some(s) = p.lock().await.remove(&id) { let _ = s.send(msg); }
                } else { let _ = tx.send(msg); }                          // notifications + server requests
            }
            let _ = tx.send(json!({"method": "$hostExited"}));
        });
        let mut errs = BufReader::new(child.stderr.take().unwrap()).lines();
        tokio::spawn(async move {                                        // diagnostics ring buffer
            while let Ok(Some(l)) = errs.next_line().await { log::warn!("muse: {l}"); }
        });
        let stdin = Mutex::new(child.stdin.take().unwrap());
        tokio::spawn(async move { let _ = child.wait().await; });
        Ok((Arc::new(Msp { stdin, next_id: Mutex::new(1), pending }), rx))
    }

    async fn send(&self, frame: Value) -> anyhow::Result<()> {
        let mut w = self.stdin.lock().await;
        w.write_all(format!("{frame}\n").as_bytes()).await?;
        Ok(w.flush().await?)
    }
    pub async fn request(&self, method: &str, params: Value) -> anyhow::Result<Value> {
        let id = { let mut n = self.next_id.lock().await; *n += 1; *n };
        let (s, r) = oneshot::channel();
        self.pending.lock().await.insert(id, s);
        self.send(json!({"jsonrpc":"2.0","id":id,"method":method,"params":params})).await?;
        let msg = tokio::time::timeout(std::time::Duration::from_secs(30), r).await??;
        if let Some(e) = msg.get("error") { anyhow::bail!("{method}: {}", e) }  // branch on e.data.kind
        Ok(msg["result"].clone())
    }
    pub async fn notify(&self, method: &str) -> anyhow::Result<()> {
        self.send(json!({"jsonrpc":"2.0","method":method})).await
    }
    pub async fn respond(&self, id: &Value, result: Value) -> anyhow::Result<()> {
        self.send(json!({"jsonrpc":"2.0","id":id,"result":result})).await
    }
}

pub enum Outcome { Completed, KeyRejected(String), Quota(i64), Failed(Value), HostDied }

pub async fn build_turn(msp: &Msp, events: &mut mpsc::UnboundedReceiver<Value>,
                        ws: &str, prompt: &str, ui: &impl BuildUi) -> anyhow::Result<Outcome> {
    let init = msp.request("initialize", json!({
        "clientInfo": {"name":"macroscope","title":"Macroscope","version":"0.1.0"},
        "capabilities": {"requestedCapabilities": ["userShell","sessionListStream"]}})).await?;
    if init["schema"]["fingerprint"] != KNOWN_GOOD_FP { ui.warn_drift(&init); } // then run smoke test
    if init["sandboxAvailability"] == "unavailable" { anyhow::bail!("sandbox unavailable") }
    msp.notify("initialized").await?;

    // Observed: a rejected/missing key at startup degrades to an empty bundled catalog.
    let models = msp.request("model/list", json!({})).await?;
    if models["source"] == "bundledCatalog" && models["models"].as_array().map_or(true, |a| a.is_empty()) {
        return Ok(Outcome::KeyRejected("catalog fetch rejected at host start".into()));
    }

    let start = msp.request("session/start", json!({
        "commandId": cmd_id(), "workspaceRoot": ws, "approvalMode": "promptUnmatched"})).await?;
    let sid = start["session"]["sessionId"].as_str().unwrap().to_owned();

    let ack = msp.request("turn/start", json!({"commandId": cmd_id(), "sessionId": sid,
        "input": [{"type":"skill","selector":"macroscope-microapp"},
                  {"type":"text","text": prompt}]})).await?;
    let turn_id = ack["turnId"].as_str().unwrap().to_owned();            // authoritative

    while let Some(ev) = events.recv().await {
        let p = &ev["params"];
        match ev["method"].as_str().unwrap_or("") {
            "item/started" | "item/updated" | "item/completed" => ui.upsert_item(&p["item"]), // replace iff higher revision
            "item/delta" => ui.append(p["itemId"].as_str().unwrap(),
                                      p["field"].as_str().unwrap_or("text"), p["delta"].as_str().unwrap()),
            "session/tokenUsage" => ui.meter(&p["modelId"], &p["usage"], &sid),
            "approval/request" => {
                msp.respond(&ev["id"], json!({})).await?;                  // receipt first, always
                let choice = ui.ask_approval(p).await;                     // user picks one of availableChoices
                if let Err(e) = msp.request("approval/decide", json!({
                    "sessionId": sid, "approvalId": p["approvalId"],
                    "requirementId": p["currentRequirementId"],             // multi-stage race guard
                    "choiceId": choice.choice_id, "feedback": choice.feedback,
                    "commandId": cmd_id()})).await { ui.refresh_approval(p, e); } // stale/resolved: refresh, don't error
            }
            "userInput/request" => { msp.respond(&ev["id"], json!({})).await?; ui.ask_question(p); }
            "turn/completed" if p["turnId"] == turn_id.as_str() => {       // match by turnId (stale terminals exist)
                return Ok(match p["terminal"].as_str() {
                    Some("completed") => Outcome::Completed,
                    _ => match p["error"]["kind"].as_str() {
                        Some("authRequired") => Outcome::KeyRejected(p["error"]["message"].to_string()),
                        _ if p["error"]["quota"].is_object() =>
                            Outcome::Quota(p["error"]["quota"]["resetsAtMs"].as_i64().unwrap_or(0)),
                        _ => Outcome::Failed(p.clone()),
                    },
                });
            }
            "$hostExited" => return Ok(Outcome::HostDied),                 // respawn + session/resume
            _ => {}                                                        // open protocol: ignore unknowns
        }
    }
    Ok(Outcome::HostDied)
}
```

(`BuildUi` is the UI-facing trait: `upsert_item`, `append`, `meter`, `ask_approval`, `ask_question`, `warn_drift`, `refresh_approval`.) The Python equivalent that produced the transcripts is `plan/probes/msp_probe.py`.

---

## 8. Observed vs. inferred: quick reference

**Observed:**
- NDJSON framing.
- The handshake, the capability grants and the protocol error frames.
- The `turn/start` ack shape, the full item sequence, `item/delta` with `field:"text"`, and `turn/foregroundCompleted` followed by reminders.
- `session/tokenUsage` and `contextUsage`.
- `authRequired` from the env key in `serve`, the logged-out fallback in a fresh home, and the `exec` behaviour for both key lanes.
- `session/userShell` working, plus `runtime_busy`.
- `goal/set` auto-continuation and auto-pause.
- `session/fork` (no cut OK; cut failed), and fork sharing the workspace.
- `session/rename`, `session/read`, `session/list` and `session/resume` across a host restart.
- Crash reconciliation.
- Sandbox network modes and `--disable-shell`.
- The key visible in the shell env.
- XDG isolation and its Keychain exception.
- Skill discovery paths and user-scope install.
- The launcher's update and login behaviour.
- `exec --base-url` and the Bearer auth.

**Schema only (not exercised):**
- The approval request/decide/updated/resolved flow, and user-input prompts.
- Quota detail and retry notifications.
- `view/gap`, `view/page`; subagent and task controls; `sideChat`; `turn/steer`, `turn/interrupt`; `plugin/install`; `session/leave`; `sessionInUse`.

**Assumptions:**
- Model shell tools inherit the env.
- Reminder agents bill on `meta`.
- `muse auth set` writes the shared Keychain item.
- The TUI takes a lease that conflicts with MSP.
- XDG on Windows.
- An installer exists; redistribution terms.

---

## Open questions for the owner

1. **Re-run `muse login`.** Probe 13 cleared your personal muse account credential (§0.3). Also confirm the overnight keeper does not depend on it, or switch the keeper to `META_API_KEY`.
2. **Provide a real key once** (as the `META_API_KEY` env var only), so the successful turn can be measured: model id, `tokenUsage`, latency, and how much the reminder agents cost per turn. Command in §0.2.
3. **Key exposure tolerance.** Accept "the agent can read the key, but has no network by default" for v1? Or wait for a key-proxy path (needs Meta to add `serve` base-URL config, or scoped/spend-capped keys)? Do you want a separate low-limit "builder" key if Meta supports it?
4. **Network for builds.** Is the default "restricted network; ask before allowing package downloads" acceptable, given it makes some builds slower or impossible without a click?
5. **Windows.** Ship Windows v1 with building disabled until Meta documents a Windows muse install, or ask Meta now?

## Decisions I made

- MSP over stdio (NDJSON JSON-RPC) is the integration; `muse exec` is only for one-shot utility calls.
- One long-lived `muse serve` per sandbox posture (build-restricted, build+net on demand, plan read-only), shared by all micro-apps; not one per app.
- Durable sessions always (fork, read and resume need them); never `--no-session-log` for builds.
- Muse runs with its own XDG home under Macroscope's app support dir.
- The key lives in Macroscope's own Keychain / Credential Manager item and reaches muse only as `META_API_KEY` in the child env.
- Never `account/*`, `muse auth set`, `muse login` or `muse logout`.
- Bad keys are detected at host start through the empty `bundledCatalog` plus stderr, and at turn time through `error.kind == authRequired`; message text is never parsed.
- Key rotation means drain, respawn, then `session/resume`. Revocation means pause everything and show a blocking banner, with no automatic retry.
- One UsageLedger fed by `session/tokenUsage`, subagent usage and the pattern engine's own responses, priced by Macroscope's own table.
- One git repo per micro-app, with a commit per completed turn; no `--trust-workspace`.
- "Rewind" = git reset plus `session/fork` (no cut). "Try another approach" = git worktree plus a new session.
- `goal/*` = an opt-in "Build until done" with $ and time budgets enforced through `goal/pause`.
- Macroscope conventions ship as a user-scope skill pack in the isolated muse home (a plugin with hooks in v1.1).
- Approval cards come from `availableChoices`, with `localPersistent` hidden, the receipt sent immediately, and the `requirementId` guard.
- An advanced terminal drawer (xterm.js + portable-pty) with a plain shell and the Muse TUI. The TUI gets the session through `session/leave`, then `muse resume`, then `session/resume` when it exits.
- Don't bundle muse. Detect it, spawn `muse-bin-<ver>` directly, and gate on the schema fingerprint plus an echo-provider conformance smoke test.
