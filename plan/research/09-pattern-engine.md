# Pattern engine: evidence before inference

Read in full: `plan/BRIEF.md`. Scope: capture output through the builder handoff; no capture implementation or generated-automation runtime. Date: 2026-10-22.

**Recommendation:** keep a local, replayable evidence pipeline. Discover repetitions with deterministic sequence matching, rank them using measured effort and verified implementation capabilities, then ask a Meta model to explain only the shortlist. A model-generated description must never manufacture repetition, saved time, or app capabilities.

**Evidence convention:** [assumption] marks proposed designs, estimates, thresholds, and untested behavior throughout this document, including illustrative JSON/SQL. These are implementation starting points, not measured performance. External implementation facts have individual source links. [local] means inspected supplied machine references, with execution claims attributed explicitly to the brief.

## 1. Pipeline and operating contract

[assumption] One Rust background worker owns ingestion and SQLite writes on both platforms. Platform adapters supply macOS AX or Windows UI Automation observations in the same vocabulary; never assume equal coverage. Windows UI Automation exposes UI elements and control patterns programmatically [verified: Microsoft, https://learn.microsoft.com/en-us/windows/win32/winauto/entry-uiauto-win32]. Windows capture and automation still require external testers; CI cannot establish real application coverage.

```text
OS/browser adapters
  -> in-memory policy gate + redaction
  -> validated events / redacted media store
  -> ordered reducer -> actions + dataflow edges
  -> episodes + overlapping task threads
  -> local sequence candidates + periodicity
  -> capability gate + scoring
  -> optional vector neighbors -> Meta explanation
  -> validated Opportunity -> user review -> builder
```

[assumption] Each stage records input revision, algorithm version, policy version, and a durable cursor. Deduplicate by `(capture_run, producer, producer_seq)`. Batch writes every 250 ms or 100 events; bound the memory queue to 10,000 events. Under pressure drop redundant frames first, then emit an explicit capture gap; never silently treat incomplete capture as uninterrupted activity. Recompute affected episodes when delayed OCR arrives. Suspend mining while locked, paused, or on excluded apps. Exclude Macroscope and its generated tools from discovery to avoid learning its own suggestions.

## 2. Event contract

[assumption] Use JSON Schema with a discriminated `kind`, reject unknown required versions, reject unexpected payload properties, and limit strings and arrays. This common envelope accompanies every payload below:

```json
{
  "schema_version": 1,
  "event_id": "ev_1042",
  "capture_run": "run_22",
  "producer": "mac.ax",
  "producer_seq": 1042,
  "occurred_at": "2026-10-22T01:02:03.456Z",
  "received_at": "2026-10-22T01:02:03.466Z",
  "mono_ms": 3123456,
  "timezone": "Pacific/Auckland",
  "kind": "ax.focus",
  "context": {
    "app_id": "com.google.Chrome",
    "process_instance": "p_18",
    "window_id": "w_7",
    "tab_id": "tab_2",
    "document_id": "doc_9"
  },
  "privacy": {"policy_version": 4, "state": "sanitized"},
  "quality": {"source": "ax", "confidence": 0.95},
  "payload": {
    "element_id": "el_amount",
    "role": "textbox",
    "native_role": "AXTextField",
    "label": "Amount",
    "label_origin": "approved_ui_label",
    "selector_ref": "sel_31",
    "secure": false
  }
}
```

[assumption] IDs are device-local opaque tokens. Window handles are scoped to process instances; element handles are scoped to document revision. A separate sanitized selector registry provides best-effort identities across reloads. `mono_ms` orders and measures durations within a capture run; UTC supports dates and recurrence. Start a new run after restart and record sleep, timezone changes, and clock jumps. A coordinator merges producer streams with a 500 ms reorder window; causal links override receipt order. Late events trigger bounded revision, never an invented total order.

[assumption] Required payload shapes follow. Samples are synthetic; `null` explicitly means unavailable. Persist only approved UI labels, never AX values, arbitrary page text, or text selected by the user.

```json
[
  {"kind":"app.focus","payload":{
    "previous_app_id":"com.apple.finder","app_id":"com.google.Chrome",
    "display_name":"Chrome","reason":"activated"}},
  {"kind":"window.change","payload":{
    "change":"url","window_id":"w_7","tab_id":"tab_2",
    "title":"Xero bill","document_id":"doc_9",
    "url":{"origin":"https://example.invalid","route":"/bills/:id",
           "resource_token":"res_8"}}},
  {"kind":"ax.focus","payload":{
    "element_id":"el_amount","role":"textbox","native_role":"AXTextField",
    "label":"Amount","label_origin":"approved_ui_label",
    "selector_ref":"sel_31","secure":false}},
  {"kind":"pointer.click","payload":{
    "button":"primary","count":1,"target_element_id":"el_amount",
    "target_observation_id":"ev_1042","target_confidence":0.95,
    "window_point":[0.42,0.31]}},
  {"kind":"key.shortcut","payload":{
    "modifiers":["primary"],"key":"C","command":"copy"}},
  {"kind":"text.burst","payload":{
    "field_id":"el_amount","selector_ref":"sel_31",
    "started_mono_ms":3123500,"ended_mono_ms":3125200,
    "typed_codepoints":6,"length_bucket":"5-8",
    "length_quality":"input_method_reported","paste_included":false}},
  {"kind":"clipboard.copy","payload":{
    "clipboard_id":"clip_4","types":["text/plain"],"byte_length":6,
    "hash":{"algorithm":"hmac-sha256","key_epoch":"2026-10-22",
            "value":"illustrative-device-local-digest"},
    "source_element_id":"cell_b2","redacted_preview":null}},
  {"kind":"clipboard.paste","payload":{
    "clipboard_id":"clip_4","types":["text/plain"],"byte_length":6,
    "hash":{"algorithm":"hmac-sha256","key_epoch":"2026-10-22",
            "value":"illustrative-device-local-digest"},
    "target_element_id":"el_amount","redacted_preview":"[NUMBER]",
    "link_quality":"matching_clipboard_generation"}},
  {"kind":"file.download","payload":{
    "operation":"ready","file_id":"file_3","directory":"Downloads",
    "name_template":"invoice_<id>.csv","path_token":"path_4",
    "previous_path_token":null,"extension":"csv","size_bytes":2048,
    "readiness":"inferred_stable","source_context_event_id":null}},
  {"kind":"frame.ref","payload":{
    "frame_id":"fr_12","artifact_id":"blob_12","width":1280,"height":800,
    "captured_mono_ms":3125300,"redaction_revision":4,
    "mask_artifact_id":"mask_12","purpose":"post_action",
    "perceptual_hash":"local-only-phash"}},
  {"kind":"ocr.ref","payload":{
    "ocr_id":"ocr_12","frame_id":"fr_12","artifact_id":"txt_12",
    "engine_version":"adapter-v1","language":"en",
    "block_count":7,"redaction_revision":4,"mean_confidence":0.91}}
]
```

[assumption] `window.change.change` is `window|title|url|tab|document`; file operations are `created|modified|renamed|ready|deleted`. Add lifecycle kinds `idle.start/end`, `capture.gap`, `lock`, `sleep`, and `policy.changed`. Idle events carry timestamps/duration; gaps carry affected producers and lost-count estimates. They are essential boundary evidence.

[assumption] Identity/privacy rules:

- Strip URL query, fragment, credentials, and unapproved path segments before persistence. Retain route templates plus keyed resource tokens, not URLs containing invoice IDs. Titles default to app/page category unless an adapter recognizes safe UI text.
- Read field role and label, never value. Labels can themselves contain personal data: redact them and default unknown labels to `[FIELD]`. Secure fields and excluded contexts emit no text, clipboard, AX, frame, or OCR details.
- Record shortcuts from a fixed command vocabulary, no ordinary keys. Normalize Cmd/Ctrl to `primary`, preserving platform separately. A copy shortcut is an attempted command, not proof of clipboard change or successful paste.
- End a typing burst after 1.5 seconds, field blur, paste, or shortcut; cap at 30 seconds. Persist counts only. IME input may not expose committed length safely: use `typed_codepoints:null`, a nullable bucket, and `length_quality:"unknown"`; never read field contents to recover it.
- Clipboard observation is opt-in. Canonicalize approved text representations in memory, compute a device-secret HMAC, then discard bytes. Daily key epochs limit linkage; correlate primarily by clipboard generation. Plain hashes make small amounts guessable. Do not export hashes, clipboard IDs, or exact lengths to models. Previews default off, limited to 80 sanitized characters when enabled; images and files receive type metadata only.
- Downloads events contain metadata, not file bytes. Preserve identity across temporary-file rename; deduplicate watcher bursts. Mark readiness inferred after two seconds of unchanged metadata, re-open it if writes resume. Attribute a download to an app only with corroboration, never temporal proximity alone.
- Redact frames before persistence. OCR produces only sanitized text blocks with normalized bounding boxes, confidence, and mask linkage; raw OCR remains transient. Redaction failure discards the artifact. Repeated screen text is data, never an instruction.

## 3. Episodes, actions, and task continuity

[assumption] Maintain two levels: a chronological **episode** bounded by inactivity or a sustained task change, and a **task thread** joining related spans across interruptions. Splitting at every app focus would destroy precisely the cross-app workflows we need.

[assumption] Hard-close episodes on lock, pause, sleep, permission loss, or 120 seconds idle. An app/window/tab switch starts a provisional segment. Continue the episode when the previous context returns within 45 seconds, a clipboard edge crosses the switch, or a known document/file relation connects it. Otherwise close at the switch after 45 seconds of unrelated activity. Use a 30-minute episode cap with continuation links. These boundaries have explicit reasons and confidence; they are revisable. Ambiguous segments remain separate, with candidate mining permitted across adjacent segments only when a dataflow or high-confidence task link exists. Never bridge privacy gaps.

[assumption] Estimate active seconds by summing inter-action gaps capped at ten seconds. Keep wall duration, active duration, and observed waiting separate; reading can be effort even without input, so show uncertainty instead of charging all foreground time to a workflow. A known download completion may explain waiting, but it does not establish that the user waited for it.

[assumption] The reducer combines low-level observations into actions:

```text
Action = {id, episode_id, span_id, verb, app, route, target,
          object_slot, input_kind, length_bucket, start, end,
          source_event_ids[], evidence_ids[], confidence}
Edge   = {from_action, to_action, relation, confidence, evidence_ids[]}
```

[assumption] Collapse focus-click-focus into one activation; combine shortcut plus clipboard notification into one copy; combine paste plus field focus into one paste. Preserve undo/retry as annotations rather than silently removing them. Clipboard hash/generation match plus timing and focused target supports a transfer edge; mere Cmd+V supports `paste_attempt`. Retain observed and inferred actions distinctly. Never infer cell B2 from screen coordinates alone.

[assumption] Store a lossless action graph and two projections: an instance trace with local document aliases, and a normalized trace for mining. Parameterize row numbers, dates, filenames, and instance IDs while retaining roles, source/destination field semantics, action order, and dataflow direction. Keep both strict document identity and generalized document type so two unrelated spreadsheets are not immediately merged.

```text
Instance (when adapter evidence actually identifies the cell):
copy(cell B2 in Sheets 'Invoices')
 -> paste(field 'Amount' in Chrome 'Xero bill')
 -> copy(cell C2 in Sheets 'Invoices')
 -> paste(field 'Reference' in Chrome 'Xero bill')
 -> activate(button 'Save draft')

Normalized:
COPY(sheet.cell.amount, row=$r, doc=$source)
 -> PASTE(bill.amount, from=$v1)
 -> COPY(sheet.cell.reference, row=$r, doc=$source)
 -> PASTE(bill.reference, from=$v2) -> SAVE_DRAFT(bill)
```

[assumption] Without adapter support use `sheet.cell.unknown`, lower confidence, and omit invented semantics. Render model traces from sanitized projections, numbered actions, relative times, and evidence IDs; never send raw event JSON or filesystem paths.

## 4. Discover repetition locally

[assumption] Run incremental exact mining every five minutes and deeper mining nightly over a rolling 28 days; retain aggregate recurrence counts for 90 days.

1. **Seed:** rolling hashes of contiguous normalized action n-grams, lengths 3-12. Require three non-overlapping occurrences, across two episodes, plus either two dates or a five-item batch. Do not count multiple overlapping windows within one execution as separate support. Batch repetitions and task executions have separate counters.
2. **Extend:** join overlapping seeds into maximal candidates up to 60 actions. Prefer closed patterns: suppress fragments with the same occurrence set when a longer pattern explains them. Remove generic navigation-only patterns such as focus/click/back.
3. **Recover variants:** bucket by app set, endpoint roles, and dataflow signature. Compare candidate windows using weighted edit distance divided by maximum alignment weight; initially accept distance <=0.20. Navigation insertions cost 0.25, target substitutions cost 1, reversed transfer or changed write destination vetoes a merge. Use medoids and require every member near its medoid; avoid transitive chaining into unrelated tasks.
4. **Handle gaps:** add constrained PrefixSpan for longer sequences after exact mining works. PrefixSpan grows sequential patterns using projected prefixes [verified: original paper, https://hanj.cs.illinois.edu/pdf/span01.pdf]. Our variant allows at most three skipped actions, 30 seconds between matched actions, ten minutes total span, and no protected boundary crossing. Retain matched intervals, not just subsequences; demand >=70% action coverage to avoid matching arbitrary browser activity. Stop at 10,000 candidates or 60 seconds per job and resume later.
5. **Consolidate:** assign stable cluster IDs using medoid and occurrence overlap. Record split/merge lineage for feedback and rebuilds. Explain coverage, outliers, and unsupported branches rather than averaging them away.

[assumption] Weekly periodicity uses local weekday/time-of-day histograms with 30-minute bins and +/-60-minute tolerance. Label “usually Tuesdays around 2pm” only after four observed weeks, >=75% matching weeks, and concentration above the user's overall activity baseline. Track recording coverage: an unobserved Tuesday is unknown, not a missed workflow. Preserve original timezone and handle DST in local time. Multiple jobs in a Tuesday batch count as one schedule occurrence. A schedule raises explanatory relevance, never establishes repetition or automatically creates a trigger.

[assumption] Embeddings are a secondary neighbor generator, not evidence. Start with local 384-dimensional signed feature-hash vectors of semantic action bigrams, roles, and app categories; L2-normalize and use cosine distance. These are reproducible structural embeddings, not a claimed Meta embedding product. Retrieve ten nearest medoids, then require sequence/dataflow validation. A learned semantic embedding provider is a later adapter, enabled only after verifying an appropriate Meta API model, dimensions, cost, and privacy. No unverified endpoint, context size, or embedding service is required to ship v1.

## 5. Friction, feasibility, and ranking

[assumption] For each cluster, compute robust per-occurrence medians over non-overlapping spans. Missing observations reduce confidence rather than becoming zeros.

```text
F = observed executions / covered workdays * 5        # weekly rate
D = median active minutes per execution
e = min(undo_or_retry_annotations / eligible_actions, 0.25)
s = min(median task-relevant app switches / 6, 1)
c = min(median verified cross-field transfers / 6, 1)
Friction = F * D * (1 + 2*e) * (1 + 0.5*s) * (1 + 0.5*c)
```

[assumption] Undo and retries are friction proxies, not proven mistakes. Count an undo followed by re-entry to the same field within ten seconds once; do not label ordinary undo as user error. Cap multipliers to prevent noisy capture dominating ranking. Show `F*D` as observed weekly effort; Friction is a ranking unit, never minutes saved.

[assumption] Maintain an app/route/OS/version **capability registry** populated by adapters and builder probes, not LLM opinion. Each essential step records supported operation, evidence, test date, permission/auth prerequisites, and confidence. Assign implementation reliability `r`: tested local file transform 0.95; documented API operation plus successful read-only/schema probe 0.90; tested application script 0.85; stable browser DOM action 0.75; tested native accessibility action 0.60; coordinate/OCR-only action 0.20; unknown 0. These numbers are priors requiring calibration, not vendor reliability claims. Browser does not automatically mean automatable; native does not automatically mean impossible.

```text
coverage = supported essential steps / all essential steps
A = coverage * min(r for essential steps)
Q = min(capture_coverage, alignment_consistency, identity_quality,
        support_strength)                 # each in [0,1]
Rank = Friction * A * Q * preference_weight
Savings = F * max(D - estimated_review_and_runtime_minutes, 0)
```

[assumption] Capture coverage is healthy required-producer seconds divided by eligible foreground seconds within matched spans; alignment consistency is one minus median normalized edit distance; identity quality is the fraction of essential targets with stable semantic selectors, weighted by adapter confidence. Preference weight begins at 1 and stays within [0.5,1.5]. Bootstrap occurrence-level savings intervals; with fewer than eight executions show a rough range rather than a precise forecast. Require at least five covered workdays before extrapolating a weekly rate; earlier batch cards use observed batch totals only.

[assumption] Support strength starts at 0.6 for three executions and reaches 1 at eight executions across three dates; cap at 0.7 for a single-day batch. Offer buildable suggestions only with complete essential-step coverage, `A>=0.60`, `Q>=0.65`, and estimated savings >=10 minutes/week, or a batch saving >=5 minutes. Schedule-specific suggestions additionally require the periodicity threshold. Unavailable capability evidence yields an internal candidate or “teach this workflow” state, never an “Automate this” promise. Prefer automating a verified subset, such as producing an import CSV, when the full write path is unsupported.

[assumption] Example: five executions/week, D=3, e=.05, s=c=1 gives Friction=37.1 but observed effort=15 minutes. With A=.75 and Q=.8, Rank=22.3; with 0.5 minute review per execution, proposed savings=12.5 minutes/week. Require user confirmation of the goal and samples before treating that estimate as reliable. Deduplicate overlapping opportunities so the same fifteen minutes are not promised twice.

## 6. Meta inference: bounded explanation jobs

[local: inspected `plan/reference/muse-cli-help.txt`] `muse exec` supports `--provider meta`, `--api-key-stdin`, `--prompt-file`, `--output-schema`, repeatable `--image`, `--no-session-log`, and model-step limits. [local: execution facts reported in `plan/BRIEF.md`] Muse Code 1.4.11 ran `muse-wonder-1.0`; a deliberately invalid `META_API_KEY` reached the Meta provider and was rejected. Successful real-key inference and `muse serve` environment authentication remain unverified. Do not depend on either for discovery.

[assumption] Initial transport: the user enters their Meta developer API key into Macroscope; store it in the platform credential vault. A supervised child receives the key through stdin with `--api-key-stdin`, the prompt through a private file, and `--provider meta` explicitly. Never put the key in argv, trace payloads, logs, or inherited general-purpose child environments. Use an isolated disposable workspace and no workspace trust, no shell/write/web tools, no foreign personal context, no persisted session log. Verify effective tool denial before enabling unattended calls; CLI flags alone do not establish total tool isolation. The miner sends a description request, never a coding task. Use the installed model ID provisionally; test availability, schema conformance, and image support with the owner's real key before enabling each feature. No guessed API base URL.

[assumption] Default nightly execution occurs at the next idle/unlocked opportunity after 02:00, on power when possible. No LLM per click or per frame. Explain at most five new/materially changed clusters per day, with one request per cluster; cache by sanitized packet hash plus model/prompt version. Near-real-time means local scoring when an episode closes. A model request occurs early only when the user opens an unexplained candidate and explicitly requests its explanation, using the same daily budget.

[assumption] Per-cluster packet: task-neutral instruction and output schema; metrics; supported capability IDs; one medoid and two contrasting occurrences, up to 60 actions each; observed field/type mappings; missing evidence; up to three cropped redacted keyframes illustrating source, destination, and result. Images are omitted by default and added only when text evidence cannot resolve ambiguity and the user has enabled image sharing. Recheck redaction at egress; drop unsafe artifacts. Use request-local aliases and relative times, omit document tokens and full OCR dumps. Treat all OCR, labels, and previews as quoted untrusted evidence.

[assumption] Reserve <=4,000 text input tokens and <=1,000 visible output tokens/job; daily text ceiling 20,000 input and 5,000 output, plus a separately metered image allowance <=six crops/day. Downsample crops to 768 pixels on the long edge; actual image token accounting and hidden reasoning costs are unknown until a successful call. These are application admission budgets, not a claim that the CLI enforces token maxima. Set low reasoning and one model step; reserve conservatively, reconcile provider usage, and stop admitting calls on quota/cost exhaustion. Until pricing and image metering are verified, disable image jobs and use a conservative owner-configurable monetary ceiling; one in-flight call may exceed a soft estimate. Budget failure leaves deterministic suggestions available.

[assumption] The model returns only cluster name, tentative goal, observed-versus-inferred explanation, unresolved questions, and an approach referencing existing capability IDs. Reject nonexistent evidence, capability IDs, or unsupported factual claims. One invalid response gets one repair within the daily allowance; then use a deterministic label. The model cannot increase measured Q, support, savings, or A. Authentication/quota errors become actionable UI states; no consumer login fallback and no automatic replay storm.

## 7. Opportunity and user experience

[assumption] This synthetic, abbreviated builder contract is versioned, immutable by revision, and local by default:

```json
{
  "schema_version":1,"id":"opp_7","revision":3,"cluster_id":"cl_9",
  "status":"suggested","title":"Prepare invoice import rows",
  "goal":{"text":"Turn invoice sheet rows into an import CSV",
          "origin":"proposed","user_confirmed":false},
  "observed_steps":[
    {"id":"s1","verb":"copy","target":"sheet.amount",
     "evidence":["action:a1","frame:fr_12"],"confidence":0.94},
    {"id":"s2","verb":"paste","target":"bill.amount",
     "evidence":["action:a2"],"confidence":0.90}
  ],
  "observations":{"executions":8,"covered_workdays":10,
                  "median_active_seconds":180,"weekly_rate":4},
  "sample_inputs":[{"fixture_id":"fx_in","kind":"synthetic",
                    "data":{"amount":"123.45","reference":"INV-001"}}],
  "sample_outputs":[{"fixture_id":"fx_out","kind":"synthetic",
                     "data":{"csv_row":"INV-001,123.45"}}],
  "apps":[{"id":"sheets.web","os":"macos"},
          {"id":"accounting.web","os":"macos"}],
  "suggested_approach":{"kind":"local_csv_transform",
    "capability_ids":["csv.map.v1"],"scope":"verified_subset",
    "manual_steps":["export source rows","review and import CSV"],
    "prerequisites":["user selects source export and confirms field meanings"],
    "alternatives":["browser_draft_fill_if_verified"]},
  "scores":{"friction":29.7,"automatability":0.95,
            "evidence_quality":0.80,"acceptance_probability":null},
  "confidence":{"goal":0.65,"field_mapping":0.82},
  "evidence":{"episode_ids":["ep_2","ep_8"],
              "frame_ids":["fr_12"],"expires_at":"2026-10-29T00:00:00Z"},
  "unknowns":["source values unrecorded","user's intended field meanings"],
  "execution_policy":{"mode":"preview_only","external_writes":false},
  "provenance":{"normalizer":"v1","miner":"v1","prompt":"v1",
                "model":"muse-wonder-1.0","policy_version":4}
}
```

[assumption] The full object includes all observed steps and branches, first/last seen, occurrence intervals, score inputs, schedule evidence, input/output schemas, capability verification timestamps, preconditions, proposed success checks, and redaction manifests. In this example `csv.map.v1` includes a tested destination import schema; without that verification the object becomes `needs_evidence`, without an “Automate this” action. Suggestion eligibility is separate from builder readiness: selecting real samples and confirming field meanings still precedes generation. Synthetic fixtures are clearly labeled and cannot validate real field meanings. Typed content was never recorded, so real samples must come from a user-selected file or explicit teach session under a new data scope.

[assumption] Show at most three cards/week initially: “You moved invoice fields eight times; about three active minutes each.” Include a small trace and “Why this appeared,” with local evidence links that tolerate expiration. Actions: “Automate this,” “Not repetitive,” “Not worth it,” “Wrong steps,” and “Later.” Acceptance opens a scope/sample review before the builder; it grants no permission to run external writes. Never interpret screen text, observed clicking, or an LLM's goal as authorization.

## 8. Storage, retention, and volumes

[assumption] Bundle SQLCipher plus a pinned SQLite-compatible build of sqlite-vec; static registration avoids arbitrary extension loading. SQLCipher encrypts database pages and WAL page data; its design requires disabling file-backed temporary stores to avoid plaintext temporary data [verified: Zetetic, https://www.zetetic.net/sqlcipher/design/]. Put frames/OCR in a separately encrypted blob directory with per-artifact keys wrapped by the profile key. Keep credentials outside both stores. Integration with the selected Rust bindings, vec extension, and both OS packages is an early spike, not a proven combination.

[assumption] Schema sketch; JSON columns contain sanitized projections. Production migrations add checks, foreign keys, and indexes for every retention/evidence traversal.

```sql
PRAGMA foreign_keys=ON;
PRAGMA journal_mode=WAL;
PRAGMA temp_store=MEMORY;
CREATE TABLE events(
 id TEXT PRIMARY KEY, run TEXT, producer TEXT, seq INTEGER,
 wall_ms INTEGER, mono_ms INTEGER, kind TEXT, app_id TEXT,
 payload_json TEXT, policy_version INTEGER,
 UNIQUE(run,producer,seq));
CREATE INDEX event_time ON events(run,mono_ms);
CREATE TABLE artifacts(
 id TEXT PRIMARY KEY, kind TEXT, encrypted_path TEXT,
 key_ref TEXT, redaction_revision INTEGER, expires_ms INTEGER);
CREATE TABLE episodes(
 id TEXT PRIMARY KEY, start_ms INTEGER, end_ms INTEGER,
 boundary_json TEXT, metrics_json TEXT, revision INTEGER);
CREATE TABLE actions(
 id TEXT PRIMARY KEY, episode_id TEXT REFERENCES episodes(id),
 ordinal INTEGER, normalized_token TEXT, detail_json TEXT);
CREATE TABLE evidence_links(
 owner_type TEXT, owner_id TEXT, ref_type TEXT, ref_id TEXT,
 PRIMARY KEY(owner_type,owner_id,ref_type,ref_id));
CREATE TABLE action_edges(
 source_id TEXT, target_id TEXT, relation TEXT, confidence REAL);
CREATE TABLE clusters(
 id TEXT PRIMARY KEY, medoid_json TEXT, metrics_json TEXT,
 algorithm_version TEXT, lineage_json TEXT);
CREATE TABLE occurrences(
 id TEXT PRIMARY KEY, cluster_id TEXT REFERENCES clusters(id),
 episode_id TEXT, first_action TEXT, last_action TEXT,
 alignment_json TEXT, active_ms INTEGER);
CREATE TABLE embedding_rows(
 id INTEGER PRIMARY KEY, cluster_id TEXT, projection_hash TEXT,
 model_version TEXT, dimension INTEGER);
CREATE VIRTUAL TABLE trace_vec USING vec0(embedding float[384]);
CREATE TABLE capabilities(
 id TEXT PRIMARY KEY, os TEXT, app_version TEXT, operation TEXT,
 verification_json TEXT, expires_ms INTEGER);
CREATE TABLE opportunities(
 id TEXT, revision INTEGER, cluster_id TEXT, status TEXT,
 object_json TEXT, PRIMARY KEY(id,revision));
CREATE TABLE feedback(
 id TEXT PRIMARY KEY, opportunity_id TEXT, revision INTEGER,
 response TEXT, reason TEXT, occurred_ms INTEGER);
CREATE TABLE jobs(
 id TEXT PRIMARY KEY, kind TEXT, input_revision TEXT,
 state TEXT, cursor_json TEXT, attempts INTEGER);
CREATE TABLE model_runs(
 id TEXT PRIMARY KEY, packet_hash TEXT, model TEXT,
 usage_json TEXT, validation_json TEXT, created_ms INTEGER);
```

[verified: sqlite-vec documentation, https://alexgarcia.xyz/sqlite-vec/features/vec0.html] `vec0` supports declared float vector columns and nearest-neighbor queries using `MATCH` with `k`. [verified: project API reference, https://github.com/asg017/sqlite-vec/blob/main/site/api-reference.md] It is pre-v1 and warns of breaking changes. [assumption] Align vec rowids with `embedding_rows.id`; never mix vector versions/dimensions. Rebuild derived vectors after normalizer changes. For this small medoid corpus, exact cosine scan in Rust is the fallback if extension packaging fails; do not add a server vector database.

[verified: SQLite, https://www.sqlite.org/wal.html] WAL allows readers alongside a writer and needs checkpoint management. [assumption] Use one writer, short reader transactions, bounded background checkpoints, and a disk-full stop state. Select a currently patched SQLite baseline compatible with SQLCipher; the WAL documentation now records a WAL-reset bug, so version selection must include its fix rather than trusting the OS library. Resume idempotent jobs after crashes and reconstruct derived tables from retained sanitized data.

[assumption] Eight-hour workday estimates, decimal MB, before database/index overhead:

| Data | Assumed daily count/size | Daily volume |
|---|---|---:|
| Events | 40,000 x 700 bytes | 28 MB |
| Actions, links, episodes, metrics | 6,000 actions plus links | 8 MB |
| Changed redacted frames | 6,000 x 80 KB | 480 MB |
| Sanitized OCR, selected frames only | 2,000 x 2 KB | 4 MB |
| Cluster vectors | 100 x 384 x 4 bytes | 0.154 MB |

[assumption] Frames dominate: 480 MB/day means 3.36 GB for seven days. Raw sanitized events: 28 days x 28 MB = 784 MB. Retain frames/OCR seven days; events/actions 28 days; sanitized medoids, aggregate recurrence counts, and feedback 90 days. Expect roughly 5-7 GB including indexes and working space, not a guarantee. Enforce a 10 GB default quota; stop frames at 750 MB/day, prune oldest eligible artifacts first, and report reduced evidence coverage. Never record screenshots at video rates for this engine. At 100 medoid vectors/day, 90-day float storage is about 13.8 MB before overhead.

[assumption] User deletion traverses evidence lineage to actions, occurrences, clusters, vectors, cached prompts, and builder handoffs; cancel queued model jobs first. Recompute surviving aggregates and rebuild derivative indexes. Do not claim secure erasure from SQL DELETE: SQLite documents limitations for virtual-table shadow data [verified: SQLite, https://www.sqlite.org/pragma.html#pragma_secure_delete]. [assumption] Profile wipe destroys local encryption keys; selective deletion needs checkpoint/compaction and cannot recall data already sent to the provider. Do not back up capture storage by default; audit Muse temporary files and crash logs for copies.

## 9. Evaluation and feedback

[assumption] Build a consented, local corpus by recording the developer doing 12 workflow families with eight executions each across six weeks: spreadsheet-to-form transfer, download/rename/import, weekly report, expense draft, file conversion, and native-app equivalents. Include exceptions, interruptions, IME typing, permission denial, and unrelated browsing. Add at least 100 negative episodes: research, creative writing, navigation, and coincidental copy/paste. Use sandbox accounts and fabricated business data. Hand-label episode boundaries, task identity, essential steps, transfers, correct automation scope, and reasonable savings. A second reviewer checks 20%; owner-only labels are useful development evidence, not a product-quality estimate.

[assumption] Generate >=1,000 reproducible synthetic trace variants from labeled traces: insert distractions, drop events, duplicate notifications, change labels/rows, invert transfers, reorder timestamps, simulate clipboard overwrite, midnight/DST, batch loops, and download renames. Plant secrets and malicious OCR instructions to test leakage and injection. Synthetic quality measures robustness, not market demand. Split by whole workflow family and time; never put perturbed copies of the same seed in train and test.

[assumption] Release gates:

- Boundary F1 >=0.85 with +/-10-second tolerance; transfer-edge precision >=0.95; false task merges <5% on held-out labeled episodes.
- Candidate precision >=0.80 and family recall >=0.60; report matching tolerance, capture coverage, and each platform separately.
- Every surfaced buildable suggestion has a verified essential-step path; manually audit unsupported capability claims with a target of zero.
- Initial field target: >50% **explicit acceptance to investigate/build** within seven days of a viewed card; also report accepted/all-viewed, accepted/all-displayed, dismissed, ignored, and pending. Seek >=100 viewed suggestions across >=10 consenting users, bootstrap uncertainty by user, and avoid claiming success from one developer's repeated cards. Builder completion, first successful run, repeat use, and measured net time saved are separate outcomes.
- Zero seeded secret leakage in retained or outbound artifacts; bounded ingestion lag <1 second, nightly mining <2 minutes on the owner's Mac at the estimated volume. These are targets to measure, not benchmarks.

[assumption] Log feedback against opportunity revision and evidence. “Not repetitive” corrects cluster membership and suppresses the same pattern until materially new evidence; “Wrong steps” requests local boundary/mapping correction; “Not worth it” lowers that user's threshold preference; “Later” snoozes 30 days without a negative label. Accept is a positive relevance signal, not proof of feasibility. Failed builds lower the affected capability prior; successful runs update measured savings after subtracting review/repair time. Keep the original baseline and note workload changes to avoid attributing unrelated speedups to automation.

[assumption] Begin with explicit rules and a Beta(1,1) acceptance prior by pattern category, using only resolved responses. Do not treat ignores as dismissals. Calibrate acceptance probabilities later with user-grouped validation; until then expose evidence-quality components rather than a spurious “92% confidence.” Hold out a small, budget-limited shadow candidate set for offline review to detect ranking blind spots without increasing notification volume.

## 10. Product-killing risks and implementation order

[assumption] The largest risk is insufficient identity: pixels and click coordinates may never reveal stable source/destination semantics. Ship a narrow adapter-supported workflow set first and abstain elsewhere. Permission loss or weak Windows coverage must be visible, not compensated for by model confidence. Privacy failures in labels, OCR, and clipboard can destroy trust; redaction and egress tests precede cloud inference. Screen prompt injection must have no route to tools or authority. Meta model availability, API-key success, provider retention/terms, quotas, and actual pricing remain launch gates; the local engine must work when inference is unavailable. Capture/helper distribution, notarization, and possible store restrictions require a separate platform review; no store eligibility is assumed here.

[assumption] Implement in order: (1) schemas, privacy gate, encrypted replay store, and teach-session fixture recording; (2) deterministic actions, boundaries, and n-gram mining with evidence viewer; (3) capability registry and ranked cards; (4) real-key Meta transport and schema-validated explanations; (5) constrained PrefixSpan and vector neighbors only when held-out recall justifies them. This keeps a solo developer's first milestone testable without an open-ended multimodal agent.

## Open questions for the owner

- May nightly analysis send sanitized traces to Meta by default after setup, or should every batch require a user action? Images should have a separate setting either way.
- Is opt-in clipboard metadata/HMAC correlation acceptable, or should v1 rely on shortcut and field evidence until a teach session?
- Which two real workflows should define the first supported adapter/capability set, and can consenting Windows testers be recruited before cross-platform claims?
- What daily API spending ceiling and local storage quota are acceptable? Proposed storage default: 10 GB.

## Decisions I made

- Discover repetition locally; use Meta to explain a bounded shortlist.
- Preserve dataflow and provenance; never retain typed contents or invent samples.
- Separate episodes from task continuity so app switches remain useful evidence.
- Gate suggestions on a verified implementation path and rank friction separately from minutes saved.
- Start with n-grams/edit distance and structural embeddings; add constrained PrefixSpan only when needed.
- Use API-key-authenticated `muse exec` first; assume neither a successful real-key call nor serve authentication.
- Keep encrypted SQLite/blob storage, seven-day visual evidence, and explicit deletion lineage.
- Treat acceptance, build success, repeat use, and saved time as distinct evaluation outcomes.
