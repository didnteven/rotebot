# ADR 0004: Open-ended tools — sources, schedules, outputs, and a run-time model step (2026-10-27)

Status: accepted. Source: owner instruction, 2026-10-27 ("summarize my calendar every morning",
"check my email each month to gather the bills info", "even if I say I want to check the weather, it
should be possible to build this").

## Context

The plan's private MVP limits tools to file-in/file-out work with no network, no schedules, and no
model at run time (plan §1.3, §7.1, §8.3). The owner wants open-ended tasks. ADR 0003 already admits
model-written programs in runtime v2; ADR 0001 §2 admits connections. This record widens what a tool
may do while keeping the plan's rule that nothing gains authority silently.

## Decision: a tool is sources → program → outputs, on a trigger

1. **Sources (read, run without asking each time once approved at save):**
   - **Connection reads**: calls to a connected service's tools, from a fixed list the user approves
     when saving (service, tool, argument template, e.g. `list_events {start: today 00:00, end: today 23:59}`).
     The host makes the calls with the stored token and passes the results to the program as data.
   - **Web fetches**: HTTPS `GET` to hosts the user approves at save (e.g. `api.open-meteo.com`), with
     templated query parameters, no cookies or credentials, response cap 2 MB, redirects only to the
     same approved host. The host fetches; the sandbox never gets network access.
2. **Program**: the runtime-v2 sandboxed program (ADR 0003) turns source data into outputs. All
   source content is untrusted data (plan §8.2).
3. **Run-time model step (optional, per tool)**: the program may ask for a model call
   (`macroscope.model.ask(prompt, data, schema)`) through the user's route (Muse plan or Meta key). The
   save screen states exactly what is sent and how often ("today's calendar events are sent to Muse
   each morning"); the outbound log records every call; Local only blocks it; budgets apply (plan §9.1)
   with a per-tool monthly cap. Output is schema-constrained and treated as untrusted.
4. **Outputs**:
   - **Local outputs run without per-run approval**: a macOS notification, a digest entry in the app
     ("Today"), and a create-only file in an approved folder (date-templated names; never overwrite).
   - **External actions** (send, post, create, modify on a connection — ADR task 13) are always
     **planned** and wait in "Needs you" for approval of the exact call; they never run unattended.
5. **Triggers**: manual, plus schedules (daily at a time, weekly, monthly on a day) evaluated by the app
   while it runs (login item opt-in, plan §3.1). A missed schedule runs once at next launch and says so.
   A tool whose sources, model step, or outputs change needs re-approval (grants reset, plan §7.4).

## What stays as in the plan

Every run gets a receipt; the outbound log shows every network and model egress; Local only and
Emergency stop halt sources, model steps and schedules; grants are per tool version; generated code
never holds tokens, keys, or network access; external writes always need approval.

## Consequences

- Google Calendar and Gmail require the owner's own OAuth client (preregistered providers); public web
  sources such as Open-Meteo need no key.
- New ship blockers before external testers: source/egress canaries (no token or key reaches the
  sandbox, approved-host enforcement, redirect and size limits), schedule correctness and missed-run
  handling, and per-tool model-step consent and budgets.
