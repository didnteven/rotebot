# ADR 0005: RegExp in runtime v2

Status: accepted by the owner on 2026-10-28 ("Admit RegExp"). Synthetic evidence
is recorded below; OS admission evidence must be rerun outside this sandbox.

## Context

The §9.3 CSV-family eval exposed generated programs using regex despite the
builder packet documenting its absence. Task 22's runtime-error rerun passed
2/3 attempts; the remaining attempt reported `RegExp are not supported` at
line 26, column 33. The correction round did not recover. The frozen-build
repetition subsequently scored 28/30; task 22 then differed only by a missing
final newline. These are separate causes: this ADR admits bounded regex;
manifest revision 2 addresses the newline comparison. Neither change upgrades
the incomplete §9.3 gate or retrospectively changes recorded scores.

Evidence: [progress](../build/PROGRESS.md) and
[frozen eval repetition](../../evals/csv-family/results/2026-10-28-muse-plan-frozen-rep1.md).

## Decision and bounds

Add rquickjs 0.14.0's `intrinsic::RegExpCompiler` and `intrinsic::RegExp` to
the custom context. Date, Proxy, weak refs, shared buffers and OS APIs remain
excluded. eval/Function and async/generator constructor links remain removed.

Regex literals have no separate pattern-length cap. QuickJS is the only
JavaScript parser processing untrusted source inside the confined child; a second
full parser solely to count literal characters would enlarge that boundary for
little gain. The existing 20 MB source/request limit and 64 MiB QuickJS allocator
bound pattern compilation, while the CPU/wall interruption and host deadline
bound execution, including catastrophic backtracking.

The hardened `RegExp(...)` / `new RegExp(...)` constructor bounds pattern strings
to 1,000 Unicode characters before native compilation, including computed patterns,
aliases, `Reflect.construct`, subclasses and the constructor reachable from a regex
literal. It captures the source getter, string conversion and iterator methods
before user code can replace them. Oversized constructor patterns throw a
RangeError (`error:runtime`); this is an execution-time check. No string-code
evaluation is introduced.

Remove `RegExp.prototype.compile` and every legacy own static, including `$1`
through `$9`, input/`$_`, lastMatch/`$&`, lastParen/`$+`, leftContext and
rightContext (including their dollar-prefixed aliases). Freeze the constructor and prototype, and preserve the
existing SDK freeze. Legacy statics cannot retain previous-match data.

The regex engine runs under the existing interrupt/allocator bounds: compilation
and matching use the existing 64 MiB heap, 512 KiB stack, 10 s CPU and 12 s wall
ceilings, with host launch deadlines unchanged. QuickJS-NG 0.16.2's
`libregexp.c::lre_poll_timeout` calls `lre_check_timeout` during backtracking;
`quickjs.c::lre_check_timeout` directly invokes the runtime interrupt handler.
The runner tests exercise CPU and wall interruption separately, and the canary
requires the worker to return `error:cpu` or `error:wall`, rather than accepting
a parent kill or a generic crash.

## Canary and memory evidence

Five regex canaries cover benign exact output (split, replace and test), legacy
statics/compile absence and freezing, dynamic 5,000-character constructor rejection
(with and without `new`, via `Reflect.construct` and a subclass), a 200,000-character
literal used once, and `/(a+)+$/` against 30,000 `a` characters followed by `!`.
The huge literal must complete successfully or return `error:memory`, within the
12 s wall budget and 64 MiB + 16 MiB RSS cap; a generic error, crash or parent kill
fails. Runner tests also accept literals over 1,000 characters, retain constructor
boundary coverage and require a clean memory error for the huge literal with a
smaller allocator cap.

All prior non-regex canaries and their expectations remain intact. The harness
checks regex-redos returns a budget error within 13 seconds and applies the
existing 64 MiB + 16 MiB RSS threshold to every memory and regex canary.
Synthetic verification passed **67/67** canaries (5 successful probes and 62
explicit denials). The huge literal completed in **0.0227 s**, peaking at
**6,602,752 bytes** RSS. Dynamic constructor rejection peaked at **5,210,112 bytes**.
ReDoS returned `error:cpu` in **10.0399 s** (10.001963 s process CPU).
Memory peaks were **74,334,208 bytes** (objects) and **71,548,928 bytes**
(typed allocation); every memory/regex peak stayed below **83,886,080 bytes**.
Final synthetic measurements are recorded in
[the canary report](../build/task17-runtime-canaries.md) and
[raw receipts](../../experiments/runtime-gate-v2/task17-synthetic-results.json).
All OS rows remain UNKNOWN until the outside rerun.

## Admission follow-up

Admission evidence **must be rerun** with
`bash experiments/runtime-gate-v2/run-canaries.sh --os`. Claude reruns this
outside the enclosing sandbox. Synthetic PASS does not provide Seatbelt
admission evidence; OS rows remain UNKNOWN here. Signed immutable release
bundle evidence remains a separate gate, as in ADR 0003.
