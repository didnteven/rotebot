# csv-family eval: Muse plan route, repetition 1 on the frozen `m4/runtime-errors` build (2026-10-28)

This is the first of three repetitions on one frozen build (runner with real runtime error
detail, commit 93435c8). Manifest, route and allowance guard are the same as before.

| Result | Count |
|---|---|
| Passed (exact bytes + hidden checks) | **28 / 30 (93.3%)** |
| Infrastructure: "Muse deadline exceeded" | 1 (task 07) |
| Wrong output | 1 (task 22: the output matched except for the missing final newline) |
| Missing attempts | 0 |

- Muse allowance went from 46% to 80% of the window and from 49% to 60% of the week. The
  weekly guard (60%) now stops further runs until the weekly reset (2026-11-02 13:00 NZDT) or
  the owner's go-ahead.
- Task 22 shows that exact-byte scoring treats a missing trailing newline as wrong. The
  acceptance tests stay frozen during a gate run. Whether to normalize one final newline is an
  owner decision for the next manifest revision.

Gate status: **INCOMPLETE**. Repetitions 2 and 3 on this build are pending.
