# Macroscope runtime v2 SDK (1.0)

Write plain JavaScript defining a synchronous global `function run() { ... }`.
No imports, dependencies, eval, Function, Worker, async work, Date, Math.random,
filesystem, process, environment, console or network APIs are available.
`import` is forbidden even in comments and strings. RegExp is unavailable; use
string operations such as split, includes, indexOf, slice and join.

- `macroscope.input("input_csv")` returns the selected UTF-8 text snapshot (the
  historical handle name applies to every text format). At most 10 MB.
- `macroscope.output(name, text)` stages exactly one UTF-8 output, at most 20 MB.
  Use the user's confirmed basename verbatim, with .csv/.tsv/.txt/.md/.json.
  This never writes a user file. The host previews and commits frozen bytes only
  after separate approval, using create-only publication.
- `macroscope.log(text)` records bounded diagnostics (1 MiB total).
- `macroscope.csv.parse(text, {required: ["column"]})` returns row objects.
- `macroscope.csv.stringify(rows, ["column"])` returns CSV with formula escaping.
- `macroscope.date.toISO(value, "MM/DD/YY")` converts a date (also supports
  DD/MM/YY, MM/DD/YYYY, DD/MM/YYYY, YYYY-MM-DD and ISO8601).
- `macroscope.money.decimal(value, {currency: "USD", decimals: 2})` returns
  decimal text using the host money parser.

JSON.parse/stringify and ordinary pure array/string/object operations are
available. SDK objects are frozen. Each run has a fresh 64 MiB JS heap,
512 KiB stack, 10 second CPU and 12 second wall ceiling. Produce deterministic
bytes and never change the input. The host checks output syntax and any example.

macroscope.connection.request(connectorId, toolName, args) records a planned call and returns undefined. It never contacts a server or returns network data. Args must be a plain JSON object, at most 64 KiB. At most five requests per run. Only explicitly selected, declared connector/tool names are allowed by the host. Requests are reviewed with the output before the trusted host executes them.
