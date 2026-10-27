# csv-family eval: Muse plan route, repetition 2 (2026-10-28)

Same manifest, route and allowance guard as repetition 1. It started after the 04:52 window
reset.

| Result | Count |
|---|---|
| Passed | **27 / 30 (90.0%)** |
| Infrastructure: "Muse deadline exceeded" | 2 (tasks 05, 14) |
| Failed: generated program rejected by the sandbox | 1 (task 22, again) |
| Missing attempts | 0 |

- Muse allowance went from 0% to 38% of the new window and from 33% to 46% of the week.
- The runner labelled task 22 `infrastructure` here too. A second classifier,
  `classify_build_failure`, had the same keyword bug, and it is now fixed and tested. The true
  class is `checks_failed`.
- Running total over repetitions 1–2: **56 / 60 (93.3%)**, counting the infrastructure failures
  in the denominator.
- Task 22 failed the same way twice. Root cause: the runner replaces every runtime exception
  with one generic message, so the correction round never sees the real error. This is being
  fixed as a product change (better runtime error detail), after which task 22 will be rerun.
