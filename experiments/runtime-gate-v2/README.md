# Runtime v2 experiment

Run `bash experiments/runtime-gate-v2/run-canaries.sh` for synthetic QuickJS
receipts. It writes `results.json` and `docs/build/runtime-lock-v2.md` and exits
nonzero unless full admission is proven. The default never runs sandbox-exec.
An outside reviewer can use `bash experiments/runtime-gate-v2/run-canaries.sh --os`.
OS setup failures produce UNKNOWN, never successful denial receipts.

`macroscope-runner` accepts exactly one UTF-8 JSON request and emits one JSON
response, with booleans in `ok`. Request limits use `memory_bytes`, `stack_bytes`,
`cpu_ms`, `wall_ms`, `output_bytes`, and `log_bytes`. Defaults are ceilings; requests
can only lower them. Source must define a synchronous `function run()`. Imports
are conservatively rejected even inside comments/strings. There is no loader.

SDK:

```js
function run() {
  const rows = macroscope.csv.parse(macroscope.input("input_csv"), {required: ["id"]});
  macroscope.output("result.csv", macroscope.csv.stringify(rows, ["id"]));
  macroscope.log("done");
}
```

Date formats are the executor's strict `MM/DD/YY`, `DD/MM/YY`, `MM/DD/YYYY`,
`DD/MM/YYYY`, `YYYY-MM-DD` and `ISO8601`. Two-digit years mean 2000–2099.
`macroscope.money.decimal(text, {currency: "USD", decimals: 2})` accepts USD/EUR/GBP
and returns decimal strings, with half-away-from-zero rounding. CSV stringify
uses the executor's spreadsheet formula escaping and LF records. CSV parse
rejects empty data, duplicate/empty headers, missing required columns, and ragged
rows. Objects returned for rows have null prototypes.

The app-core seam is behind `generated-runtime`; the product retains csv-mapping.
`RunnerEngine::stage` uses the OS launcher. The separate
`synthetic-runtime-tests` feature exposes `stage_synthetic` for fixtures, with no
fallback from production sandbox failures. The synthetic launcher applies CPU,
file size, descriptor, process and core limits but omits Seatbelt and RLIMIT_AS:
finite RLIMIT_AS is unsupported on this macOS. The production path applies all
limits and refuses to launch if any fails. Neither the feature nor manifest
variant admits a generated tool to product use.

The end-to-end test lives in `crates/runner/tests/integration.rs` so Cargo supplies
the actual child executable; it drives app-core → child → broker stage → approve
→ commit, checks the independent orders fixture and create-only behavior.

This is an unsigned developer spike. OS compatibility, finite process address
space, release signing/bundle evidence and Windows isolation remain admission
gaps; see the generated lock report. The source hashes and crate checksums are
recorded to make reviewer reruns comparable.
