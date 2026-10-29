# Record a task — Stage B

Stage B adds a memory-only recording service, Tauri/TypeScript contracts, and the
five-screen desktop flow. Read [Stage A](recording-task-draft.md) and its
[TaskDraftV2 schema](task-draft-v2.schema.json) for the unchanged masking, consent,
evidence validation and answer-application guarantees.

## Public API

AppCore owns the session map; IPC uses the following snake_case command names.
TypeScript exposes their camelCase equivalents. IPC arguments use camelCase
(`sessionId`, `payloadHash`, `source`, `frame`, `answers`).

| Command | Input | Result |
| --- | --- | --- |
| `recording_status` | none | `{ captureAvailable: false }` until Stage C |
| `start_recording` | `source: "sample"` or `"window"` | opaque session ID; window capture currently refuses |
| `submit_frame` | session ID, `RecordedFrameDto` | masked frame list; consumes ingress through Stage A's `record` gate |
| `list_recording_frames` | session ID | PNG base64, sequence, dimensions, masks, dropped sequence/reason receipts and stopped flag |
| `stop_recording` | session ID | same list, with ingress closed |
| `preview_recording` | stopped session ID | exact masked gallery, cleared text JSON, system instructions, schema, exact text fields, bytes, recipient, model, estimate/caveat and payload hash |
| `consent_and_analyze` | session ID, reviewed payload hash | validated TaskDraftV2, including 2–4 questions |
| `answer_recording_questions` | session ID, question ID to option ID map | plain-English goal from Stage A `apply_answers`; every question needs a valid answer |
| `approve_recording_draft` | session ID | normal TaskDraft from `draft_task_no_input`, with optional example applied through `update_task` |
| `discard_recording` | session ID | idempotent discard; cancels pending analysis |

Raw ingress is Deserialize-only: `seq`, `monoMs`, `appName`, `windowTitle`,
`width`, `height`, `rgba` (length-checked byte array), `axLabels`, `ocrBoxes`
(`text`, `rect`), `masks` (`rect`, `reason`: `secure_field` or `manual`),
`uncertain`, `secureInputGap`. Rectangles have `x`, `y`, `width`, `height`.
RGBA decoding rejects more than 1600×1600×4 bytes; dimensions and exact byte
length are checked again before the Stage A gate. This DTO is an ingress
boundary, never a persistence or logging format. Submitting external frames
removes built-in sample authority; arbitrary ingress cannot be labeled synthetic
or receive the offline sample analyzer.

TaskDraftV2 and cost estimates retain Stage A snake_case fields. Other recording
DTO fields use camelCase. Frame listing includes no title, OCR or AX text;
cleared text is available only in the explicit preview.

## Review, egress and lifetime

The UI shows the exact masked PNGs, cleared text, instructions and schema before
Send. The service hash binds the Stage A immutable payload hash to selected and
effective route, model, egress, endpoint and policy epoch. Route or policy changes
invalidate stale consent. Only Send calls `consent_and_analyze`; defaults shown
in the question form are submitted explicitly on Looks right. Invalid/missing
answers clear prior clarification so approval cannot reuse old answers.

Local-only refuses analysis, including the sample. With cloud allowed and the
offline demo selected, the explicit synthetic sample is parsed locally through
`ModelRequest::parse_draft`, with no outbound row or cost. API-key and custom
endpoint routes use the existing gateway's redirect-free, proxy-free,
bounded-time HTTP client, with a tool-free multimodal request, 8,000 output
tokens, JSON schema and a 1 MiB response bound. Exact reviewed system/schema/
cleared-data text is recorded before dispatch under purpose `Recording analysis`,
with the recipient. Image/base64 bytes are never written to history. Custom
endpoint calls retain the existing monthly cap. History contains cleared
recording text, deletable through the existing privacy controls. Actual provider
image/schema compatibility and billing need live validation; estimates exclude
transport/base64 overhead and later builds. Muse plan image analysis refuses
with guidance to select an API model: its current adapter cannot send images
without staging files or native tools. There is no silent fallback from that route.

At most four sessions exist at once, each with 60 kept frames, 150 attempts,
64 MiB cumulative RGBA ingress and Stage A's five-minute monotonic-frame bound.
The cumulative byte budget includes dropped frames. Sessions expire after 30
minutes without API activity; access sweeps the map, and one weak background worker per AppCore
also sweeps every 30 seconds, including synchronous Tauri callers. Discard, emergency stop, data deletion and quit drop sessions;
pending cloud calls observe cancellation and the egress barrier. Recording
pixels have no disk fallback. Analysis consumes the reviewed packet and clears
the gallery before storing only TaskDraftV2. Transport failure requires discard/
restart; consumed consent is not silently replayed.

Approval composes supported steps, variables, constants, output, selected answers
and explicit unsupported/manual limits into a goal using Stage A `apply_answers`.
Its 2,000-character limit prevents builder truncation. Only this approved goal/
example enters ordinary draft persistence; frames do not. The returned TaskDraft
enters CreateFlow at Check the plan. Existing build consent, access approvals,
checks, result preview and save are reused. The previous metadata observer
remains under Advanced: app activity log.

## Native IPC enrollment pending a scope exception

The recording commands are implemented and registered in `src-tauri/src/main.rs`.
The existing app ACL also requires enrollment in `apps/desktop/src-tauri/build.rs`
and `apps/desktop/src-tauri/capabilities/main.json`. Both files are outside the
allowed edit paths for this task, so they remain unchanged pending explicit
approval. The [prepared minimal ACL patch](recording-stage-b-acl.patch) adds only
the ten recording commands and grants them to the existing main window. Until
that patch is authorized and applied, Tauri refuses the new IPC commands; the
browser/mock flow is fully usable. No wildcard or alternate IPC bypass is added.

## Stage C requirements

Stage C must supply the native capture and permission boundary before returning
`captureAvailable: true` or admitting `source: "window"`:

- A signed Swift helper using the system window picker and ScreenCaptureKit,
  with per-session authority for exactly the selected window. No whole-display
  fallback or ambient browser access. Exclude private windows; unknown private
  status closes admission rather than guessing from titles.
- A clear macOS Screen Recording / Accessibility permission flow, cancellation,
  retry/settings guidance, and secure lifecycle IPC tied to this session. A
  renderer-supplied DTO or sample ID alone is not native capture authority.
- Vision OCR with complete, bounded boxes and accurate scaling to submitted
  pixels; AX secure-field/manual mask geometry; secure-input-gap detection.
  Incomplete OCR, missing/stale geometry, uncertain private status or detection
  failures must set uncertainty/drop the frame, never claim it is safe.
- Monotonic frame ordering, pixel/metadata/session limits, capture stop barriers,
  permission revocation, picker cancellation, app quit and helper crash cleanup.
  Stop capture before review and keep all copies in memory only.
- Native evidence for password/card/private-window exclusion and geometry, plus
  live image-model/schema interoperability and cost measurement. Stage B's
  made-up sample is not evidence that native capture is safe or shipped.

## Verification

Use the external Stage B `CARGO_TARGET_DIR` and `CARGO_INCREMENTAL=0`; never
create a worktree target directory. Run app-core tests with all features, desktop
`cargo check`, TypeScript and Vitest. Dependency symlinks in this worktree point
to a read-only sibling checkout: `vitest run --configLoader runner --no-cache`
avoids writing bundled config/cache there. Localhost transport tests require
sandbox permission to bind ports; report that restriction separately from test
assertion failures. No native capture, external model requests or real credentials
are needed for these tests.

The frontend verification passed TypeScript and all 161 Vitest tests, including
recording → review → explicit Send → questions → normal build/result/save. Plain
desktop `cargo check` initially encountered the absent bundled runner sidecar;
checking with `TAURI_CONFIG='{"bundle":{"externalBin":[]}}'` succeeded without
changing bundle configuration on disk. The runner was built into the specified
external target directory for app-core runtime test fixtures. The all-features
Rust run includes sandbox-denied localhost tests; the recording service's
non-network assertions pass. The full test output is in
`/tmp/macroscope-stage-b-tests.log` for this session.
