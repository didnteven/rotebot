Review done — read-only, no commands run. I found **8 concrete defects plus 1 minor fragility**, all in the new schedules/Today and connection/model-step paths.

## 1. Web-source API keys in query strings are allowed, sent, and persisted in the outbound log

- [websource.rs](crates/connections/src/websource.rs) `parse_url` (lines 40–53), [program.rs](crates/builder/src/program.rs) `web_source_host` (lines 108–145), [web.rs](crates/app-core/src/web.rs) `log_url` (lines 49–51) and `record_fetch` (lines 91–108)
- What goes wrong: credential rejection only covers URL *userinfo* (`user@host`). A declared source like `https://api.example.com/v1?api_key=SECRET&date={today}` passes validation, is fetched as-is ([websource.rs:103-109](crates/connections/src/websource.rs)), and the full URL including the secret is stored as `payload_text` in the outbound log and shown on the Privacy screen. Payloads linger 7 days ([lib.rs:3281](crates/app-core/src/lib.rs)).
- Trigger: save a tool with a web source whose query string contains `api_key`, `token`, or `secret`.
- Fix: reject key/secret/auth-like query parameters at save (`web_source_host`/`validate_web_sources`), and log only host + path in `log_url`.

## 2. Same-host redirect check allows https→http downgrade and port change

- [websource.rs](crates/connections/src/websource.rs) redirect policy (lines 61–75) and final check (lines 125–132)
- What goes wrong: both compare only `host_str` (lowercased). A 301 from approved `https://a.test/x` to `http://a.test/x` or `https://a.test:8443/x` is followed, so an approved-host fetch can silently go over plaintext or an unexpected port, and that response becomes unattended file output.
- Trigger: approved host returns `301 Location: http://<same-host>/...`.
- Fix: require same scheme (`https`) and same effective port in the policy and the final-URL check.

## 3. Monthly model budget window is UTC while the per-tool cap counter is local-month

- [model_step.rs](crates/app-core/src/model_step.rs) lines 106–112, [source_reads.rs](crates/app-core/src/source_reads.rs) lines 6–9, [lib.rs](crates/app-core/src/lib.rs) lines 1538–1544; already flagged as an open nit in [PROGRESS.md:399-400](docs/build/PROGRESS.md)
- What goes wrong: the spend window is built as `{local-YYYY-MM}-01T00:00:00Z` parsed as UTC, while the call counter key uses the local month. East-of-UTC zones get a ~1-day window where spend counts toward the wrong month in one ledger but not the other, letting monthly spend overshoot the $25-equivalent cap.
- Trigger: in `Pacific/Auckland`, run model steps at 2026-11-01 00:30 local (2026-10-31 11:30Z): counted in the local-November cap but the UTC-October budget.
- Fix: compute month start in `schedule_timezone()` (local midnight), not UTC.

## 4. Connection-source day boundaries fail on midnight DST transitions while schedules advance

- [source_reads.rs](crates/app-core/src/source_reads.rs) lines 47–59 vs [schedules.rs](crates/app-core/src/schedules.rs) lines 122–132
- What goes wrong: `source_args` errors (`"Source boundary falls in a timezone gap"`) when `{today_start}`/`{month_start}` (00:00) is unrepresentable, but `occurrence()` handles gaps by advancing to the first valid minute. In zones with midnight spring-forwards (e.g. `America/Santiago`), every connection read fails for that day while the schedule itself still fires.
- Trigger: tool with a connection source using `{today_start}` on the transition day in such a zone.
- Fix: mirror `occurrence()` — advance to the first valid minute instead of erroring.

## 5. Unattended model prompt is program-chosen bytes; approval covers only the purpose string

- [model_step.rs](crates/app-core/src/model_step.rs) lines 150 and 213 (`json!({"prompt": r.prompt, "untrusted_data": r.data})`, sent verbatim up to the 8 KiB / 64 KiB caps)
- What goes wrong: save-time consent covers `model_step.purpose` (a short human sentence, [program.rs:70](crates/builder/src/program.rs)), but each scheduled run sends whatever prompt the generated code passes to `macroscope.model.ask`. A scheduled run can exfiltrate an arbitrary prompt plus fetched calendar/email bodies to Meta/Muse without the user ever seeing that text.
- Trigger: builder-produced code whose `ask()` prompt diverges from the approved purpose (e.g. purpose "summarize calendar", prompt asks for verbatim personal details).
- Fix: bind the exact prompt template at save (hash into the grant, re-approve on change) or surface the literal prompt in the save review.

## 6. Muse-plan model input (system + all source data) is passed as child-process argv

- [exec.rs](crates/muse-client/src/exec.rs) lines 218–225 (`.arg(prompt)`), prompt built at [model_step.rs:213](crates/app-core/src/model_step.rs)
- What goes wrong: the full model input including calendar/email bodies goes on the `muse` command line, visible to other local processes via `ps` and capturable in crash/diagnostic reports. The codebase already treats argv as sensitive (key goes via stdin, and there is a canary asserting the *key* never appears in argv, [exec.rs:403-447](crates/muse-client/src/exec.rs)) — but fetched data gets no such protection.
- Trigger: any model step via the Muse plan route.
- Fix: pass the prompt via stdin or a workspace file, like the schema file already is.

## 7. Model answers have no byte bound; schema allows unbounded `maxLength`

- [model.rs](crates/runner/src/model.rs) `validate_schema` (lines 39–101) places no bound on `maxLength`/`minItems`/`maxItems`; [model_step.rs](crates/app-core/src/model_step.rs) lines 68–75 inserts the answer with only a schema-match check
- What goes wrong: the 64 KiB discipline on model *inputs* is bypassed on the return path — e.g. schema `{"type":"string","maxLength":20000000}` is under 8 KiB of schema text yet permits a ~20 MB answer. The host holds it and replays it; the runner's 192 KB answers check ([lib.rs:265-269](crates/runner/src/lib.rs)) only fails the run *after* the $2 budget reservation and monthly-cap slot were consumed (reserved/incremented before dispatch, [model_step.rs:119-144](crates/app-core/src/model_step.rs)).
- Trigger: schema with huge `maxLength` plus a compliant model answer.
- Fix: cap answer bytes (e.g. 64 KiB) before inserting into `answers`, and bound `maxLength`/`minItems`/`maxItems` in `validate_schema`.

## 8. OAuth-token echo is rejected on the read path but not on the commit path

- [mcp.rs](crates/connections/src/mcp.rs): `call_tool` (lines 264–283) rejects results echoing the bearer token, but `call_approved_tool` (lines 253–260) has no such check; commit path at [connections.rs:592-599](crates/app-core/src/connections.rs) stores the full `value.to_string()` as the action summary, which lands in the receipt (broker truncates to 2000 chars, [effect-broker/lib.rs:782-810](crates/effect-broker/src/lib.rs)) and is rendered as receipt text.
- What goes wrong: a compromised/misbehaving MCP server that echoes the access token in an action result gets that token persisted in the receipt row and displayed in the UI. (The journal is safe — actions are cleared there, [effect-broker/lib.rs:904-907](crates/effect-broker/src/lib.rs).)
- Trigger: approved connection action whose server result contains the bearer token string.
- Fix: apply the same token-echo rejection in `call_approved_tool`.

## 9. (Minor) Run status derived by substring matching on error text

- [schedules.rs](crates/app-core/src/schedules.rs) lines 745–750 (`body.contains("Reconnect")` → `needs_you`, `contains("Skipped")` → `skipped`), same pattern at lines 458–465 and 571–577
- What goes wrong: status is inferred from message substrings rather than typed errors. `needs_you` entries are also exempt from the 7-day Today expiry ([schedules.rs:404-406](crates/app-core/src/schedules.rs)), so a future failure message containing "Reconnect" would persist indefinitely. No current trigger found (only intended messages match today), but the coupling is fragile.
- Fix: thread a typed status/reason from `stage_inner` instead of re-parsing the message.

## Checked and found clean (no defect)

Double-run protection (watermark persisted before fetch, [schedules.rs:648-655](crates/app-core/src/schedules.rs); `schedule_eval` mutex; `claim_tool_run` + busy snapshot); planned connection calls never auto-commit ([schedules.rs:714-718](crates/app-core/src/schedules.rs), `commit_run` refusal at [lib.rs:3011-3017](crates/app-core/src/lib.rs)); schedule binding covers program/hosts/folder identity with re-approval ([schedules.rs:231-272](crates/app-core/src/schedules.rs)); model-request keys bound to exact prompt/data/schema with 3-request/3-round caps ([model_step.rs:18-81](crates/app-core/src/model_step.rs), [sdk.rs:320-354](crates/runner/src/sdk.rs)); connection reads revalidate live hints, schemas, and substituted args before every call ([connections.rs:794-868](crates/app-core/src/connections.rs)); web 2 MB + connection 2 MB response caps; model outbound log holds byte counts only ([model_step.rs:146-149](crates/app-core/src/model_step.rs)); notifications capped at first output line / status text ([schedules.rs:546-587](crates/app-core/src/schedules.rs)); DST ambiguous→earliest with single firing, gap→advance, month-end clamp ([schedules.rs:120-172](crates/app-core/src/schedules.rs)); Local-only/epoch rechecks around fetches ([lib.rs:2813-2828](crates/app-core/src/lib.rs)); `today_local()` (process-TZ shell-out) is currently test-only ([web.rs:162](crates/app-core/src/web.rs)) — all live paths use the injected timezone.