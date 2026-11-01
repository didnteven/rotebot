# Macroscope bot-themed UI/UX plan

## Scope and review basis

Review of the React UI and CSS in `apps/desktop/src`, including App, Today, CreateFlow, RunFlow, Connections, History, and shared UI helpers. This is a source-based review; rendered screens and native macOS interactions still need visual verification. No UI implementation is included in this plan.

## Design direction

Make Macroscope feel like a workspace of small, capable robot assistants. Each saved task gets its own recognizable bot, used consistently during creation, in My tools, during runs, and beside results. Take the approachable, expressive character direction requested for Muse/Grok as inspiration, while creating an original Macroscope character system.

Use calm neutral surfaces, strong typography, a restrained accent, rounded controls, and expressive bot faces. Keep the workspace useful for everyday work: large illustrations belong in onboarding and creation; small avatars belong in lists and activity. Results and approval requests remain the main content.

Keep current product terms such as “task” and “tool” initially. A bot is the visual identity of a tool, so users do not have to learn a second object model.

## Findings and proposed improvements

| Priority | Current observation | Proposed improvement |
| --- | --- | --- |
| P0 | API waits commonly display “Working…” or plain loading text. Some create steps can render duplicate busy messages. | One contextual status surface per operation, with a bot animation and a specific label such as “Preparing your task” or “Testing connection.” |
| P0 | `useAction` exposes one busy boolean per caller; in App and Today that state is shared across multiple unrelated controls. | Track operation identity and affected task/connection, distinguish initial load from refresh, and disable only conflicting controls. Guard duplicate submissions. |
| P0 | Build phase and events exist, but the main build message collapses most activity into “Building and checking your tool…”. | Show backend-driven stages, completed steps, elapsed time, and the current action. Clearly distinguish working from waiting for approval. |
| P0 | My tools rows expose many competing actions including export, pause, delete, edit, and run. | Give Run primary emphasis, provide a clear task detail/edit entry, and collect management actions in an accessible overflow menu. Keep destructive actions separated and confirmed. |
| P1 | Tasks have text identities only; the empty state uses a generic symbol. | Assign each task a stable bot portrait; replace the empty symbol with a welcoming bot and useful starter tasks. |
| P1 | Buttons mostly differ only by primary/quiet styling, with a border-only hover change. | Introduce consistent primary, secondary, ghost, and danger buttons with loading, pressed, hover, focus, and disabled states. |
| P1 | The global status strip gives model routing significant persistent space. | Use a compact model/cloud status control with expandable details; retain clear warnings for unavailable access and local-only mode. |
| P1 | Today uses large repeated result surfaces; task and result hierarchy can compete. | Make Needs your OK the first group, use bot identities and status badges, and show concise result previews with expansion. |
| P1 | Connections can show “Loading connections…” whenever the supplied list is empty. | Model loading, empty, failed, refreshing, and connected states explicitly. Add local feedback for sign-in and connection tests. |
| P1 | Technical detail disclosure is common, but creation still presents many choices. | Use clearer section hierarchy and progressive disclosure for advanced options; keep required source/access decisions visible. |
| P2 | Theme follows the OS; existing components use many similar text sizes and surfaces. | Add System/Light/Dark preference, a consistent spacing and typography scale, quieter borders, and clearer surface elevation. |
| P2 | Responsive layout exists, but task action groups wrap freely. | Design compact task cards and deliberate action placement for narrow windows; validate at 320px and with zoom. |

## Task bot identity and artwork

- Create a small coherent family of original bots with distinct silhouettes, faces, accessories, and accent palettes. Start with data, research, writing, communication, and general-purpose motifs.
- Every task receives its own combination of features, not just a category icon. Seed the identity when the draft is created and persist it when saved; preserve it through task renames, edits, and version restores.
- Persist a compact, versioned avatar descriptor with the task, including palette, face, body, accessory, and variation seed. Ensure uniqueness within the local library, with a fallback variation for collisions. Define identity behavior for imported, duplicated, and legacy tools.
- Let users choose another bot or shuffle its appearance. Keep this optional and out of the required creation path.
- Use layered SVG or similarly lightweight local artwork for the first release: it scales cleanly and supports face/arm animation without another API wait. A generated bitmap portrait can be a later optional enhancement with an immediate local fallback.
- Use approximately 32–40px avatars in compact rows, 64–80px on cards, and 144–192px in the creation progress panel. Check legibility in both themes.

## Animation and loading behavior

| State | Bot behavior | Supporting UI |
| --- | --- | --- |
| Initial screen load | Small waking/looking animation | Layout-matched skeletons and “Loading your tools…” |
| Draft request pending | Bot looks at task input | “Preparing your task…”; retain the entered request |
| Building | Bot assembles from parts, then works | Real build stage and event summary; existing Stop build action |
| Testing | Eyes scan or a small tool moves | “Checking the result…” and completed stage indicators |
| Waiting for approval | Bot pauses with an attentive expression | “Needs your OK”; prominent source/action review and approval controls |
| Running | Task’s own bot works in place | Specific operation label; results stay visible where applicable |
| Saving | Short settling animation | “Saving your tool…” on the initiating button |
| Success | One brief nod/check gesture | Persistent textual success and result/open-output action |
| Failure | Calm concerned expression | Plain-language cause, recovery action, preserved input, optional details |
| Paused/stopped | Static resting pose | Explicit Paused or Stopped badge |

Animation must follow actual state. Do not rotate invented stage messages or show percentage completion without measurable progress. Map actual backend build phases to user-facing stages; label request-only waits honestly when finer progress is unavailable.

Give immediate button feedback; delay skeletons/large loading surfaces around 150–250ms to avoid flashing. For a long wait, show elapsed time and a neutral “This is taking longer than usual” message after a configurable threshold. Keep existing content visible during background refresh. Today’s 30-second refresh should not repeatedly animate the entire screen or announce loading.

Use gentle opacity/transform motion and short transitions. Animate the active bot, rather than every library avatar. Stop loops when work ends or the view is hidden. With reduced motion, show static expressive poses and text. Decorative art should be hidden from screen readers; announce meaningful status changes once through a polite live region and use `aria-busy` on the affected region.

Expose Cancel/Stop only where the backend supports it. In particular, cancelling a loading view must not suggest that a committed external action has been undone. Do not blindly retry operations whose outcome is unknown; preserve the existing receipt/reconciliation behavior.

## Screen-by-screen changes

### My tools and app shell

- Add a small original brand bot mark, navigation icons with text, clearer active navigation, and a compact status control.
- Replace action-heavy rows with scannable bot cards or structured rows: portrait, name, short purpose, state, last run, and Run.
- Put Edit, Schedule where available, Export, Pause/Resume, and Delete in a consistent management area. Add search once the library becomes large enough to need it.
- Offer a friendly first-run bot, one clear create action, and starter examples. Preserve the existing sample workflow.

### Create a task

- Present Describe → Review → Build & test → Save as a clear journey, adapted to the existing flow rather than forcing extra clicks.
- Introduce the task bot alongside the draft, then use that same identity during build and save.
- Use a dedicated progress panel with current phase, completed stages, optional event details, and Stop build.
- At access approval, stop the working animation and clearly state what is waiting for the user. Keep source review and outbound-data approval explicit.
- On success, put the useful result first, followed by task naming and Save. Add a brief bot completion gesture without delaying review.

### Run and results

- Keep the bot and task name visible throughout input selection, preview, approval, execution, and receipt.
- Replace generic busy text with labels specific to previewing, checking, running, and opening output.
- Make Preview, Approve and run, and output actions visually distinct at their respective steps. Preserve reviewed-action requirements.
- Preserve useful previews on recoverable errors. Show the recovery action close to the failure.

### Today, Activity, Connections, Settings, and Privacy

- Reuse the same task portraits in Today and Activity so users recognize which assistant produced each result.
- Use consistent state badges with text and icons; never communicate state through color alone.
- In Connections, use service branding for the service and a small bot only for assistance/status. Show “Waiting for sign-in in your browser” separately from active API work.
- Group settings into clearly titled cards with inline save/test feedback and a theme preference.
- Make privacy status easy to scan. Keep emergency stop and delete-data actions prominent, clearly labeled, and visually distinct from routine actions.

## Implementation sequence

1. **Design specification and representative mockups.** Produce light/dark My tools, creating, waiting-for-approval, result, and error screens. Define bot silhouettes, motion frames, spacing/type/color tokens, and button states. Deliver a reviewable visual baseline.
2. **Shared theme and feedback primitives.** Update `styles.css` and `ui.tsx`; introduce Button, StatusBadge, Skeleton, LoadingState, and an operation-aware action helper. Apply consistent feedback to existing actions before adding decorative motion. Keep components in the desktop app until there is a second consumer, consistent with `packages/ui/README.md`.
3. **Stable bot identity.** Extend task/draft contracts and storage with a backward-compatible avatar descriptor; carry it through the Rust/Tauri seam and mock backend. Add local bot artwork and renderer, migration/fallback behavior, and a small avatar picker.
4. **Creation and execution.** Add BotProgress and phase-to-presentation mapping to `CreateFlow.tsx` and `RunFlow.tsx`. Preserve source approval, action review, stop behavior, failure recovery, and receipts.
5. **App-wide polish.** Apply bot cards and action hierarchy in `App.tsx`; update Today, History, Connections, Schedule, Settings, recording, and privacy flows. Add compact layouts and theme preference.
6. **Visual and interaction QA.** Verify the real desktop app plus mock scenarios with slow, failed, empty, approval, stopped, and successful responses. Tune animation and spacing based on rendered screens.

Suggested first delivery: theme/button primitives, contextual loading, and a complete bot-assisted create/run journey. Then extend the same system to library management and secondary screens.

## Acceptance criteria

- Every awaited user action has visible, specific feedback; fast actions avoid large loading flashes, and background refreshes preserve readable content.
- Each task has a recognizable, persistent bot identity across creation, library, execution, and results.
- Actual backend state drives progress, including approval, stopped, failed, and ready states. No fabricated completion percentages.
- Duplicate submissions are prevented without unnecessarily disabling unrelated tasks. Slow or stale responses cannot overwrite newer operation status.
- Keyboard navigation, visible focus, existing 44px control targets, screen reader status, contrast, and reduced motion remain supported.
- Bot animation works in both themes, does not shift layouts, and stops when inactive. Narrow-window layouts and long task names remain usable.
- Source approval, destructive-action confirmation, local/cloud visibility, and unknown-outcome handling retain their existing behavior.
- Add focused interaction coverage for delayed/rejected requests, status transitions, duplicate submissions, identity persistence, and approval boundaries. Use rendered visual checks for theme and layout rather than tests that only mirror CSS.

## Outstanding visual validation

Before implementation is finalized, inspect rendered screens in the native app. This review identifies source-visible structure and state gaps; exact spacing, contrast compliance, animation quality, and window behavior require visual QA.

## Implementation status — 2026-10-31

Implemented on `m4/custom-endpoint`, based on `65561aa` (newest local branch tip; no remote configured):

- Original local SVG bot family with palette, head, accessory, and body-mark variations; optional shuffle during task creation.
- Draft identity is carried to the saved task in local browser/webview storage. Legacy/imported tasks fall back to a deterministic task-ID identity. This is device-local, not an exported/backend avatar field.
- Animated creation, build, checking, waiting-for-access, running, success, and failure poses, with reduced-motion support.
- Contextual bot status panels in the main loading flows, model/API settings, and connection setup; explicit sign-in waiting state.
- Violet/neutral light and dark surfaces, appearance preference, task portraits, card treatment, button interaction states, primary Run action, and responsive adjustments.
- Immediate duplicate-submission protection in the existing shared action helper.
- Connections now distinguishes initial loading from an empty loaded list.
- Interaction tests cover identity, safe artwork, theme preference, and duplicate submissions. Existing renderer safety checks validate trusted local bot artwork separately from untrusted content.

Remaining follow-up work from the larger roadmap: rendered/native visual QA (no browser connected in this session), backend/export persistence for avatars, operation scoping beyond each existing action-helper caller, skeletons and slow-request timing, management overflow menus, library search, and the deeper per-screen layout redesign. No native/backend approval or receipt behavior was changed.

### Character artwork revision

Replaced the first geometric SVG characters after visual feedback. The app now uses
an original transparent 3D companion atlas with four distinct material/silhouette
families and six expression poses each. Existing identity seeds remain stable;
state changes select actual pose artwork in addition to motion. The asset is
bundled locally in `apps/desktop/src/assets/bots/companions-v2.png`.
