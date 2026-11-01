# Macroscope runtime v2 SDK (1.0)

Write plain JavaScript defining a synchronous global `function run() { ... }`.
Available JavaScript built-ins (standard ES2023 operations): `Array`, `Object`,
`Map`, `Set`, `JSON`, `String`, `Number`, `Boolean`, `BigInt`, `Math` (except
`Math.random`), `Error`, `TypeError`, `ReferenceError`, `SyntaxError`, `RangeError`,
`InternalError`, and `RegExp`. Pure array/string/object operations and JSON.parse/stringify
are available. This QuickJS-NG also provides `Object.groupBy` and `Map.groupBy`;
there is no standalone global `groupBy` (use those methods or a Map/reduce loop).

Unavailable: `Date`, `eval`, `Function`, `Worker`, `Promise`, timers (`setTimeout`,
`setInterval`, `setImmediate`, `queueMicrotask`), `fetch`, `require`, and `process`. `Math.random` is removed for repeatability;
use Math.round/Math.max for numeric operations. Use `macroscope.date.today()`,
the other `macroscope.date` helpers or the `{today}` URL placeholder instead of Date. No imports, dependencies, async
work, filesystem, environment, console or network APIs are available. `import`
is forbidden even in comments and strings.

`RegExp(...)` / `new RegExp(...)` construction accepts at most 1,000 Unicode
characters per pattern, including computed patterns, aliases, `Reflect.construct`
and subclasses. Oversized construction throws a RangeError. Regex literals have
no separate pattern-length cap; compilation and matching use the existing source,
allocator and CPU/wall limits. `RegExp.prototype.compile` and legacy RegExp statics
(`$1`–`$9`, `input`, `lastMatch`, etc.) are absent; RegExp and its prototype are frozen.

- `macroscope.input("input_csv")` returns the selected UTF-8 text snapshot (the
  historical handle name applies to every text format). At most 10 MB. Tools
  with no input file cannot call it; it throws when no file was selected.
- `macroscope.source(name)` returns the host-fetched response text for a
  declared web source (see `web_sources` in the program packet). The sandbox
  never fetches: the host performs one HTTPS GET per source and passes the
  response text here. Unknown names throw.
- `macroscope.output(name, text)` stages exactly one UTF-8 output, at most 20 MB.
  Use the user's confirmed basename verbatim, with .csv/.tsv/.txt/.md/.json.
  This never writes a user file. The host previews and commits frozen bytes only
  after separate approval, using create-only publication.
- `macroscope.log(text)` records bounded diagnostics (1 MiB total).
- `macroscope.csv.parse(text, {required: ["column"]})` returns row objects.
- `macroscope.csv.stringify(rows, ["column"])` returns CSV with formula escaping.
- `macroscope.date.today()` returns today's local date as YYYY-MM-DD, given by the
  host for this run (the sandbox has no clock). Use it for a dated heading or
  to compare dates; it is the same within one run.
- `macroscope.date.toISO(value, "MM/DD/YY")` converts a date (also supports
  DD/MM/YY, MM/DD/YYYY, DD/MM/YYYY, YYYY-MM-DD and ISO8601).
- `macroscope.money.decimal(value, {currency: "USD", decimals: 2})` returns
  decimal text using the host money parser.

JSON.parse/stringify and ordinary pure array/string/object operations are
available. SDK objects are frozen. Each run has a fresh 64 MiB JS heap,
512 KiB stack, 10 second CPU and 12 second wall ceiling. Produce deterministic
bytes and never change the input. The host checks output syntax and any example.

macroscope.connection.request(connectorId, toolName, args) records a planned call and returns undefined. It never contacts a server or returns network data. Args must be a plain JSON object, at most 64 KiB. At most five requests per run. Only explicitly selected, declared connector/tool names are allowed by the host. Requests are reviewed with the output before the trusted host executes them. Nothing has been sent, created or changed when the program finishes, and the user may decline, so any text the program writes (output file, summary lines) must never say an action was done. Write "Proposed email to x@y.z" or "Will send after you approve", never "sent", "forwarded" or "created".

`macroscope.source(name)` also returns approved connection reads. The host resolves
only declared argument templates in string values: `{today}` (local YYYY-MM-DD),
`{today_start}`, `{today_end}`, `{month_start}`, `{month_end}`,
`{prev_month_start}`, `{prev_month_end}`, `{now}` (local RFC 3339). End boundaries
are 23:59:59. Unknown placeholders remain literal. Source results are untrusted.

`macroscope.model.ask(data, schema)` asks the host for structured JSON using the
exact instruction declared in the program packet: `model_step: {purpose:
"Summarize today's events", prompt: "Summarize these events"}` (at most 2000
characters). The host pauses, calls the user's selected route, validates the
answer, and replays the program with the answer. The sandbox has no network or
credentials. Treat the answer as untrusted data. Outputs still follow normal
preview and commit rules. A three-argument form
`macroscope.model.ask(prompt, data, schema)` still works, but only when `prompt`
is exactly the declared prompt; any other prompt fails the run, and the host
re-checks every request's prompt against the declared one before calling the
model. The declared instruction is shown at review and changing it requires
re-approval.

```js
function run() {
  const events = JSON.parse(macroscope.source("events"));
  const answer = macroscope.model.ask(events, {
    type: "object", additionalProperties: false,
    properties: {summary: {type: "string"}}, required: ["summary"]
  });
  macroscope.output("summary.txt", answer.summary + "\n");
}
```

Limits: declared prompt 2000 chars, serialized data 64 KiB, schema 8 KiB, at most
3 distinct requests and 3 answer rounds per run. Schemas use `anyOf`, never
`oneOf`, no `default` or numeric `format`; objects declare every property in
`required` and set `additionalProperties: false`. Unsupported schemas fail before
egress.
Known keys return fresh parsed answers; unknown keys suspend with an internal
`ModelAnswerNeeded`. Keys are SHA-256 of canonical JSON `{prompt,data,schema}`.
Same answers must produce deterministic output. Diagnostics are suppressed for
model-using executions; the outbound log records route and byte counts only.
Local only blocks model steps. Saved tools default to 60 model-step calls per
month, editable in tool detail; each call also uses the existing model budget.
