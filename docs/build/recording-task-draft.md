# Recording to Task Draft v2 — Stage A

Stage A is a headless, in-memory Rust API in `app-core::recording`. It adds no
capture, permission prompts, UI, model calls, persistence or builder wiring. The
[metadata observer experiment](observer-experiment.md) remains separate and does
not demonstrate pixel capture or its signed-bundle shipping gates.

## Flow and authority

1. In the future capture adapter, the user starts a session and chooses a window
   with the system picker. User-picked browser windows are allowed; private
   windows remain excluded. Unknown private-window status must close admission;
   title heuristics do not establish that a window is public. There is no ambient
   browser permission or whole-display fallback. Stage A accepts synthetic frames
   and does not implement this native selection/exclusion boundary.
2. The adapter supplies `RecordedFrame` (sequence, monotonic milliseconds, app,
   sensitive title, dimensions, memory-only RGBA, AX labels and bounded OCR boxes),
   secure-field/manual `MaskRegion`s, and uncertainty/secure-input-gap flags.
   `RecordingSession::record` consumes raw frames. Geometry must be nonempty and
   wholly inside the frame; stale geometry or incomplete OCR/detection must set
   uncertainty or return a detector error. No AX secure-field geometry means
   uncertainty, not permission to proceed. Caller adapters are responsible for
   accurate geometry, complete OCR, session authority and lifecycle barriers.
3. The gate detects email, Luhn-valid card numbers, phone, long mixed alphanumeric
   token/key, IBAN-like and SSN-like text in OCR. It burns opaque black fills into
   RGBA **before** PNG encoding. It drops uncertain/secure-input-gap frames and
   detector, geometry, pixel or encoder failures, recording only sequence/reason.
   Original title text is always omitted. OCR intersecting any mask is omitted;
   all AX labels are omitted when any mask exists because labels lack geometry.
   Otherwise sensitive AX labels are omitted, and sensitive app names replaced.
   A detector error in metadata also drops the frame. There is no raw-content
   serialization or debug implementation, disk fallback, or unmasked encoder.
4. `RecordingSession::preview(EstimateRates)` creates immutable `RecordingPayload`:
   session ID, exact masked PNG bytes and SHA-256 hashes, byte sizes, dimensions,
   system instructions, JSON response schema, cleared text JSON, total bytes,
   estimate inputs and estimate caveat. Total bytes sums UTF-8 system/schema/data
   and binary PNG sizes, excluding provider envelopes/base64 overhead. Cost uses
   caller-supplied micro-dollar rates per million tokens, image-token assumptions,
   four text bytes per token and an output-token allowance. It is a planning
   estimate, not a quote/cap, and excludes later retries/build rounds.
5. A review surface must show those exact images/text and ask for explicit
   per-session cloud consent. Only `preview.consent(ExplicitConsent::Approved {
   preview_hash })` yields `ConsentedPayload`; decline, stale or altered previews
   fail. The hash covers session, all packet content, sizing and estimate inputs.
   Only `ConsentedPayload::into_model_request` produces a packet and rechecks its
   binding. Private fields and no deserialization prevent reconstructing a cleared
   frame/consented packet from arbitrary bytes. New evidence needs a new preview
   and consent. Stage A has no transport; a future gateway adapter must preserve
   these exact contents, tool-free/schema-constrained roles, model output bound,
   Local-only policy and reviewed recipient/model. Provider request formatting
   and actual billing remain unverified.
6. The tool-free analyzer returns the draft plus 2–4 frame-grounded multiple-choice
   questions, displayed after the breakdown. Each has an explicit default and
   exactly one `any_is_fine` option. Defaults are displayed choices, not implicit
   answers. `ModelRequest::parse_draft` validates JSON and rejects unknown frame
   references. Frame pixels, titles, OCR, AX labels and app names are untrusted
   evidence in separate JSON/attachment fields, never interpolated into system
   instructions. JSON escaping prevents embedded delimiter text from creating a
   new field or role; separation is not proof that a model resists injection.
7. `apply_answers(&draft, &BTreeMap<question_id, option_id>)` requires one valid
   answer per question, rejects extra/missing/unknown choices and produces a
   `ClarifiedTask { description, example_output }`. Manual steps and unresolved
   ambiguities remain visible. It rejects descriptions over 2,000 characters to
   avoid truncation by the existing builder. The user reviews/edits and approves
   this text; answer application does not grant build or execution authority.
8. Future wiring passes approved text to the existing `draft_task` backend with
   no input file (`AppCore::draft_task_no_input`), sets the optional example through
   the existing draft edit path, and obtains native approvals for web sources,
   connections and any files. Existing checks, correction and review rounds do
   the build. Never pass raw recording/OCR into builder instructions.

## Task Draft v2 contract

The machine-readable [JSON schema](task-draft-v2.schema.json) is also included
verbatim in the analyzer packet. All objects reject unknown properties. Required
fields are `schema_version: 2`, `goal`, `steps`, `variables`, `constants`,
`output_description`, `trigger`, `unsupported_steps`, `ambiguities`, `questions`.
`example_output` is an optional string/null, containing only a synthetic example.

Steps are ordered from 1 without gaps, have unique IDs, `app`, `action`,
`evidence_frames` sequence IDs and exactly one capability:

| Capability | Runtime boundary |
| --- | --- |
| `file_read` | Read explicitly selected files only |
| `web_source` | Read approved sources only; never infer a grant from a URL |
| `connection` | Read through approved connections only |
| `model_step` | Model transformation inside existing model policy/budget |
| `output_file` | Exactly one create-only output file |
| `manual` | Unsupported step performed by the user; never automated UI control |

`variables` describe what changes per run (`name`, `description`); `constants`
hold fixed choices (`name`, `value`). Names are unique across both collections.
`output_description` describes the single resulting file. `trigger` is descriptive,
not a schedule grant. `unsupported_steps` contain `step_id`, `reason` and must
match all and only manual steps. Other-app navigation, typing and clicks cannot
be represented as supported execution; the prompt and user review enforce this
semantic boundary, while typed capabilities and the existing runtime enforce
actual available effects. A model assigning a false capability is not authority.

Each question has unique `id`, `prompt`, nonempty `evidence_frames`, 2–6 options
(`id`, `label`, `any_is_fine`) and a required `default` matching an option ID.
Rust validation additionally enforces ordering, ID/default/reference membership,
one output, manual-step coverage, one any-is-fine choice, nonblank text without
control characters, and UTF-8 byte bounds (4,000 per text, 8,000 per example).
There are at most 50 steps, variables, constants and ambiguities. The parser does
not prove that prose is truthful or safe; host checks and approval remain required.

Sessions admit at most 150 frame attempts over 300 seconds, frames at most
1600×1600 pixels, 64 KiB total metadata, and 1,024 OCR boxes, labels or supplied
masks each. Drop receipts are bounded to 150 entries. This is not an OCR engine
or universal PII detector: names, medical details, novel secrets and missed OCR
are not reliably detected. Production capture and real-model behavior remain
unverified until later stages and the plan's capture gates are met.
