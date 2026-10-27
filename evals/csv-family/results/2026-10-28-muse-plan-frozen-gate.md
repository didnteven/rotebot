# §9.3 private generated-execution gate: Muse plan route, frozen `m4/runtime-errors` build

All three repetitions ran on one frozen build (runner with real runtime error detail, commit
93435c8), manifest revision 1 (exact bytes), route `muse_plan` (muse-spark-1.3), 2026-10-28.

| Repetition | Passed | Infrastructure | Generation failures |
|---|---|---|---|
| 1 | 28/30 | 1 (07: Muse deadline exceeded) | 1 (22: missing final newline) |
| 2 | 28/30 | 2 (14, 29: Muse deadline exceeded) | 0 |
| 3 | 28/30 | 1 (07: Muse deadline exceeded) | 1 (22: RegExp not supported) |
| **Total** | **84/90 (93.3%)** | 4 | 2 |

Infrastructure failures stay in the denominator. There were zero forbidden effects and zero
missing attempts.

**Verdict: PASS.** The gate requires ≥75% bounded hidden-test success across ≥30 distinct
tasks × 3 repetitions with zero forbidden effects.

Scope (plan §9.3): this admits the one CSV family on the Muse plan route. It is synthetic
corpus evidence, not natural-utility or retention evidence, and it says nothing about the
Meta API-key route, which is still unfunded.

Follow-ups:
- Both task 22 causes are addressed on `m4/regexp-retention`: RegExp is admitted (ADR 0005)
  and manifest revision 2 ignores one final newline. A future gate run on that build needs its
  own three repetitions.
- Four of 90 builds hit "Muse deadline exceeded". Measure whether the Muse build deadline is
  too short for the slower builds (137 s was the slowest pass).

Allowance: the window reset at 09:55. The two repetitions used about 24% of the week
(60% → 84%), staying under the owner-approved 85% cap.
