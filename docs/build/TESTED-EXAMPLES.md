# Tested example inventory

Reviewed 2026-11-02 against repository evidence and newer local web-scenario logs.

## What the evidence means

- The CSV evaluation used 30 distinct tasks, three repetitions on a frozen Muse-plan build, exact expected output and hidden checks: **84/90 passed (93.3%)**, with zero forbidden effects. See [the gate report](../../evals/csv-family/results/2026-10-28-muse-plan-frozen-gate.md).
- The newer web logs include **100 explicit file-created records across repeated trials**. These are repeated attempts, not a count of unique tools. Some earlier logs only report OK and an answer, so their file creation is not counted.
- A web build passing its host checks and creating a file does not independently establish answer accuracy, relevance, source authority, or freshness. Several logged answers use unsuitable sources or proxies. Current prices and other values must not be inferred from historical logs.
- The web-scenario driver uses temporary data folders, so these tools do not all remain in the desktop library. See [the driver](../../crates/app-core/examples/web_scenarios.rs).

## Examples with recorded file creation

| Area | Examples seen in successful file-creation runs |
| --- | --- |
| Weather and environment | Auckland forecast; London weather; Queenstown weekend forecast; Wellington sunrise/sunset; Sydney and Christchurch UV; Christchurch PM2.5; Piha wave height; Auckland moon phase |
| News and reading | BBC, RNZ and NZ Herald headlines; Rust blog latest post; Lex Fridman latest episode; Wikipedia featured article |
| Developer information | Latest React, Node.js LTS, Python, Rust and Linux kernel versions; VS Code release; GitHub stars and issue counts; newest Rust questions on Stack Overflow |
| Rates and prices | NZD/AUD and USD/JPY exchange rates; Bitcoin/Ethereum prices; gold/silver prices; Apple share price |
| Geography and calendars | Japan and New Zealand population; Germany's neighbouring countries; remaining NZ public holidays; Tokyo time; days until Christmas |
| Other source-sensitive trials | Petrol prices, movie popularity, Steam top sellers, trends, electricity spot prices, RBNZ cash rate, Lake Taupo water level, solar eclipses; file creation is recorded for these, but the logs also contain failures or source/recency concerns |

The earlier real-model smoke tests additionally document a committed ERROR-line log filter, orders export and live Auckland weather file. See [the progress record](PROGRESS.md#what-works-now).

## Raw web-run evidence

The source logs were temporary local artifacts outside the repository. Their reviewed counts and SHA-256 checksums are preserved in [the portable evidence summary](web-scenario-summary.json); raw logs are not included. `OK` is the driver's own execution result. Only explicit `file: created` lines are counted as recorded file creations; repeated attempts remain separate.

| Log | OK records | Explicit file creations | Build failures | Run failures |
| --- | ---: | ---: | ---: | ---: |
| `stress-meta.out` | 23 | 23 | 2 | 0 |
| `stress-modal.out` | 24 | 0 | 1 | 0 |
| `stress2-meta.out` | 15 | 15 | 0 | 2 |
| `stress2-modal.out` | 16 | 16 | 1 | 0 |
| `stress3-meta.out` | 14 | 14 | 2 | 0 |
| `stress3-modal.out` | 11 | 11 | 4 | 1 |
| `stress4-meta.out` | 8 | 8 | 2 | 0 |
| `stress5-meta.out` | 3 | 3 | 2 | 0 |
| `stress6-petrol.out` | 2 | 2 | 0 | 0 |
| `suite-meta.out` | 8 | 8 | 1 | 1 |
| `suite-modal.out` | 9 | 0 | 1 | 0 |

Explicit file creations across these logs: **100**. These historical runs are not a fresh execution against the current app build.
