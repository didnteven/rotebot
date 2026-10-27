# ADR 0003: Programs through runtime v2 (2026-10-27)

Status: accepted for developer-boundary admission. Supersedes ADR 0002 §1's
product exclusion of generated code; its Muse binary decision still applies.

Runtime v2's admission evidence is recorded in `docs/build/runtime-lock-v2.md`.
The product enables `generated-runtime` by default in app-core and desktop.
Generated programs execute only in the one-shot QuickJS runtime v2 child through
RunnerEngine: bounded pipes, empty environment, mandatory macOS Seatbelt profile,
QuickJS allocator/stack limits, CPU/wall budgets, kill and reap. There is no
production in-process or unsandboxed fallback. Explicit synthetic execution is
available only through the test feature and test API.

This admits developer builds. Release admission still requires a signed immutable
app bundle, runner and launcher, with the release boundary/canary receipts.
Ad hoc bundling is developer packaging, not release admission evidence. Other
platforms fail closed until separately admitted.

The finite CSV mapping remains the preferred path for CSV tasks it can express.
Other selected UTF-8 text tasks use model-written synchronous JavaScript and the
pinned SDK. Inputs are at most 10 MB; one confirmed output basename must end in
.csv, .tsv, .txt, .md or .json. Only selected-input read and one create-only output
capabilities are granted. Math.random and Date are unavailable. Imports, eval,
Function, Worker, OS access, network and arbitrary dependencies remain forbidden.

The host invents structural text samples and never sends real input values.
An optional user-provided example is explicitly real data in outbound review and
requires the review checkbox. Programs require Muse or an API key; there is no
offline program generator. The host independently runs each candidate twice,
checks exact output name, deterministic bytes, nonempty UTF-8, selected output
syntax, ordered example lines and source integrity before enabling save.

Saved programs use the same stage → preview → approve → create-only commit and
receipts as mappings. Generated code exits before review; approval commits the
frozen digest without executing it again. Source, runtime/profile/SDK pins and
limits bind the staged plan. Code and input/output comparisons render as inert
text under the existing renderer CSP.
