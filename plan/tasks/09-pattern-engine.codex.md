Track: PATTERN ENGINE (from raw events to ranked automation opportunities). Output: plan/research/09-pattern-engine.md
Design the data pipeline end to end, concretely enough to implement:
- Event schema (JSON) for: app focus, window/URL change, AX focused element + role + label, click on element, key shortcut (no content), text-entry burst (length only + field identity), clipboard copy/paste (type + hash + optional redacted preview), file events (Downloads), frame refs, OCR text refs.
- Sessionization into "episodes" (boundaries by idle time, app/task switch), and an "action trace" representation that is compact and model-friendly (e.g. `copy(cell B2 in Sheets 'Invoices') -> paste(field 'Amount' in Chrome 'Xero bill')`).
- Repetition detection without an LLM first: sequence mining on normalized action traces (n-gram / PrefixSpan / edit distance), periodicity (weekly Tuesday 2pm), then embeddings + LLM to name and explain clusters.
- Friction score formula (frequency x duration x error/undo rate x context switches x copy-paste count) and an "automatability" score (are the apps scriptable? APIs? browser vs native?), so we suggest only things we can actually build.
- When and how the LLM is used (batch nightly vs near-real-time), token budgets, what context is sent (text traces + a few redacted keyframes).
- The "Opportunity" object handed to the builder: goal, observed steps, sample inputs/outputs, apps involved, suggested approach, confidence, evidence links to frames.
- Evaluation: how to build a test set (record your own workflows; synthetic traces), precision of suggestions (target: >50% of suggestions accepted?), and the feedback loop from dismiss/accept.
- Storage: SQLite schema sketch + vector index (sqlite-vec?) and data volumes per day.
