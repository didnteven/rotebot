# csv-family eval: Muse plan route, repetition 1 (2026-10-28)

Route `muse_plan` (muse-spark-1.3 via the owner's Muse Code Everyday Usage plan), manifest as
committed in `evals/csv-family/manifest.json`. One fresh build per task, run in six batches of
five with an allowance guard (stop at 75% of the window or 60% of the week).

| Result | Count |
|---|---|
| Passed (exact bytes + hidden checks) | **29 / 30 (96.7%)** |
| Failed: generated program rejected by the sandbox | 1 (task 22, group-count) |
| Infrastructure failures | 0 |
| Missing attempts | 0 |

- Median build-and-run time was 53 s (range 40–137 s), with one model call per attempt.
- Muse allowance went from 24% to 62% of the window and from 20% to 33% of the week.
- Task 22: the program failed in the sandbox with the "use synchronous SDK code only" error.
  The one automatic correction round failed too.
- The runner first labelled this attempt `infrastructure` because the sandbox message contains
  the word "unavailable". That classifier bug is fixed (`eval::classify_failure`, with a test),
  and the attempt is a `checks_failed` generation failure. The denominator is unchanged.

Gate status (plan §9.3: ≥75% over ≥30 tasks × 3 repetitions): **INCOMPLETE**. Repetition 1
passes the threshold, and repetitions 2 and 3 are not run yet. Replayed results are never
mixed in.
