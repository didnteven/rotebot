# Rotebot visual upgrade plan

## Direction

Make Rotebot feel like a premium workshop for your personal robot crew. The visual story is: describe a chore, watch your bot build a tool, review something useful, keep that bot ready for next time.

Interpret “epic” as stronger composition, tactile characters, deliberate depth, and memorable transitions. Concentrate the spectacle in the library, building, and completion moments. Keep reading, access review, and action approval calm and precise.

This plan is based on the current React/CSS source and existing UX/artwork documentation, not a rendered app inspection. The first milestone must establish actual screenshots before deciding exact spacing and contrast. No implementation is included.

## Current foundation

- The desktop app is branded Rotebot. It already has light/dark tokens, violet accents, rounded surfaces, responsive rules, reduced-motion support, and a short sidebar.
- `Bot.tsx` already supports stable task identities, expressive built-in poses, and custom artwork. Build on that investment.
- My tools already has Run, Schedule, an overflow menu, descriptions, source information, and schedule/last-run text. Improve their composition rather than add duplicate controls.
- Creation already has a stepper and contextual bot status. The opportunity is to turn these pieces into a coherent scene.
- Today already groups approvals first. Make that priority more visible and make finished results easier to scan.
- `styles.css` contains successive theme/component overrides. Consolidate the affected rules as part of the redesign so later changes remain predictable.

## Visual specification

| Element | Proposed treatment |
| --- | --- |
| Palette | Retain violet as the signature. Use warm near-white and graphite surfaces; reserve mint for verified success and amber for attention. Tune final values against rendered contrast. |
| Depth | Three clear surface levels: workspace, content, floating controls. Use soft shadows and fine edges; a restrained radial light can sit behind a featured bot. |
| Typography | Keep native system fonts. Target 36–40px page titles, 22–24px section headings, 16px body, and 13–14px metadata. Keep long result text at reading size. |
| Spacing | Use a 4/8/12/16/24/32/48px scale. Give headers and major transitions breathing room; keep repeated rows compact. |
| Shape | Standardize on roughly 12px controls, 18px cards, and 24px hero surfaces; use pills for statuses and source chips. |
| Characters | Approximately 72–96px in library cards, 160–192px during creation/build, and 40–48px beside activity. Keep custom artwork legible with consistent padding and grounding. |
| Motion | 120–180ms control feedback, 200–280ms panel transitions, one 400–600ms completion gesture. Animate the active bot only. Reduced motion uses immediate state changes. |

## Screen changes

### 1. My tools: make the crew feel present

- Give the header a clear hierarchy: “My tools,” a short useful subtitle, a primary Describe a task button, and secondary Record a task.
- Build a responsive card layout for small libraries, with a compact list treatment for larger collections. Determine the switch from populated mockups rather than an arbitrary count.
- Each card has a small portrait stage, tool name, up to two description lines, exact source/service labels, schedule or last-run information, and a stable action footer.
- Keep Run strongest, Schedule secondary, and management in the existing overflow menu. Allow names and source details to wrap without moving actions unpredictably.
- Use subtle hover elevation and a tinted portrait backdrop to distinguish identities. Keep ordinary library bots still.
- Give the empty library one larger bot and a few concrete starter requests that populate the existing create flow. Keep the sample workflow available.

### 2. Create and build: make progress tangible

- On wide windows, pair a generous task composer with a companion preview. Stack them in narrow windows.
- Organize source choices as a deliberate supporting section beneath the request. Keep the main next action visually anchored.
- Present the reviewed plan as a concise explanation followed by input/output facts and access details. Preserve the exact data review and outbound approval controls.
- Create a dedicated build stage: the chosen bot, the actual current phase, completed stages, elapsed time, and expandable event details. Map these to real backend events; do not invent progress percentages or achievements.
- Visually distinguish working, waiting for user input, stopped, and failed states. Keep Stop or recovery actions close to the status they affect.

### 3. Results: give the useful outcome the reveal

- Make the output preview the largest content surface. Put the bot and a concise state heading beside it rather than above a long stack of notices.
- Separate “Preview ready,” “Needs your OK,” “Tool saved,” and “File created.” Celebrate only the event that actually happened.
- For files, show a readable file tile with the real filename and the existing supported output action. For planned external actions, keep the exact review and Approve and run controls prominent.
- Use a brief bot gesture and restrained highlight on success. Leave the result visible after the animation ends.
- Give failures equal layout quality: plain cause, preserved context, specific recovery, optional technical details.

### 4. Today and the shell: carry the same language through

- Give Needs your OK a distinct section surface and count derived from real entries. Keep approval buttons and action details together.
- Render completed work as compact result cards with bot identity, time, outcome preview, and available output action. Expand longer content deliberately.
- Add a coherent local icon set to navigation while retaining text labels. Refine selected state, sidebar spacing, and branding.
- Apply the shared surface/type/button system to Connections, Activity, recording, Settings, and Privacy after the core journey is established.
- Preserve the current conditional setup warning; do not introduce a permanent status bar merely for visual balance.

## Delivery sequence

| Milestone | Work and likely files | Reviewable deliverable | Rough effort |
| --- | --- | --- | --- |
| 1. Visual baseline | Run the app/mock backend; capture library, create, build, approval, success, and failure. Review existing assets. | Current screenshots plus light/dark proposals for library and build/result at desktop and narrow sizes. | 0.5–1 day |
| 2. Foundation + library | Consolidate relevant tokens/rules in `styles.css`; refine shell and My tools in `App.tsx`; reuse `Bot.tsx` and shared `ui.tsx` patterns. | A complete populated and empty library with responsive layouts and consistent controls. | 1–2 days |
| 3. Signature journey | Refine `CreateFlow.tsx`, `RunFlow.tsx`, bot status presentation, and result surfaces. Verify available phase/event information before designing the timeline. | Describe → review → build → preview → approve/save, including slow and failed cases. | 2–3 days |
| 4. App consistency | Apply established patterns to `Today.tsx`, `Connections.tsx`, `History.tsx`, `Show.tsx`, `Settings.tsx`, and `Schedule.tsx`. | Secondary screens share the same visual hierarchy and state language. | 1–2 days |
| 5. Visual QA | Native macOS and mock walkthroughs, accessibility, layout and motion tuning, focused regression checks. | Screenshot set and a short acceptance checklist with issues resolved. | 0.5–1 day |

Effort is an initial estimate of roughly 5–9 working days for one implementer, assuming existing artwork and backend capabilities suffice. New illustration or missing progress contracts would be separate scope.

The first useful delivery is milestone 2: shell, theme consolidation, and My tools. Then complete the build/result journey before polishing secondary screens.

## Acceptance checks

- A populated library, active build, approval, and finished result each have an obvious focal point and next action.
- The same tool remains recognizable across library, build, run, and Today, including custom bots and unavailable-artwork fallback.
- Check native windows at 1440×900 and 1024×768, plus narrow layouts down to 320px and 200% zoom. No clipped actions or overlapping text; dense data may scroll inside its own region.
- Validate light/dark and system appearance, readable contrast, keyboard focus/order, existing 44px primary control targets, screen reader status, and reduced motion.
- Exercise empty, single-tool, and 20-tool libraries; duplicate/long names; long descriptions; unavailable connections; slow loading; waiting for approval; stopped builds; success; and recoverable errors.
- Existing content remains readable during background refresh. Motion stops when idle or hidden and never delays an action.
- Preserve exact source/action review, local/cloud visibility, destructive confirmations, receipts, and unknown-outcome recovery. A more dramatic presentation must not imply an action has completed before confirmation.
- Run desktop typecheck and relevant interaction tests after implementation. Add focused tests only for changed behavior, such as starter requests, stage mapping, or expansion; use rendered checks for visual decisions.

## Scope boundaries

Use existing bundled artwork and CSS for the first pass. No new runtime image-generation dependency, bot storage redesign, scheduling changes, or backend behavior changes are needed to achieve the visual direction. Avoid unmeasured claims such as time saved. Any metrics displayed must come from real available data.

## First implementation — 2026-11-02

- My tools now has larger bot portrait stages, identity-tinted cards, aligned action footers, a compact treatment for libraries over eight tools, and a crew welcome scene.
- Creation now uses a dedicated workshop scene with the selected bot, host-driven stages, timestamp-based elapsed time, live status, and the existing Stop action. Retries move back to the active stage; failed/stopped builds keep later stages pending; Review remains awaiting the user.
- Latest event details appear during active builds. Finished builds keep the existing single technical disclosure and recovery/review controls.
- The result reveal and broader app-wide polish remain future work. No native/backend behavior changes were made for this pass.
- Rendered/native visual verification remains outstanding: the local preview server starts, but the Browser runtime reports no connected browser.

## App-wide polish — 2026-11-02

- Added bundled outline navigation icons, a more deliberate brand treatment, selected navigation surfaces, page descriptions, and a consistent heading/spacing scale.
- Consolidated the old green and violet theme definitions into warm light and graphite dark palettes, with shared surface, status, danger, and floating-layer tokens.
- Connections now has service marks, category labels, and explicit connection-state badges. Settings separates service selection, setup, preferences, and app information. Activity has event-type icons, timestamps, and tool bot identities on receipts. Today uses the same status treatments and distinguishes approval work.
- Standardized form surfaces, feedback notices, focus treatment, menus, typography, and subtle panel transitions; existing reduced-motion rules suppress the new motion.
- Desktop typecheck and all 248 interaction tests passed. Rebuilt the native app, verified its bundle signature, and reopened it.
- Native window screenshots were inspected for the populated light library and a dark creation form at the normal desktop window size. This supersedes the earlier lack of rendered verification for those views. Narrow-window layouts and secondary-screen screenshots remain to be checked. Native capture works independently of the unavailable browser connection.

## Task composer — 2026-11-02

- Creation now has the contextual page title “Create a tool” (or “Edit tool”), a compact three-stage progress bar, and one composer card containing the larger bot, request, optional sources, and primary footer action.
- Clear/Undo are quiet text actions; selected service chips retain their checkmarks and pressed state with a restrained selected surface. File review, service selection, and character customization keep their existing behavior.
- Desktop typecheck and all 248 interaction tests passed. Rebuilt the native bundle. The currently running app is left open to preserve the unfinished request; the composer loads after quitting and reopening the rebuilt bundle.
- The new composer still needs native screenshot verification after relaunch.

## Matching review layout — 2026-11-02

- The Review step now shares the composer card, larger task-bot heading, section spacing, surface treatment, and primary action footer.
- Task summary/editing, clarification questions, output settings, access setup, outbound-data review, and build blockers remain in their existing order with the same controls and behavior.
- Typecheck and all 248 interaction tests passed. The native bundle was rebuilt; the running review is kept open until the user is ready to relaunch. Native screenshot verification of this revision remains outstanding.

## Clearer file and service selection — 2026-11-02

- Replaced the mixed source-pill row with a separate file attachment area and connected-service selection cards. Each service shows an explicit selection box and “Select to include” / “Included in this task” text while retaining its existing pressed-state semantics and five-service limit.
- Added a separate Manage connections action, an attached-file name/removal row, and plain guidance that these inputs can be left empty. Existing file preview and service-selection behavior remain covered by the interaction suite.
- Typecheck and all 248 tests passed; rebuilt the native app. Native screenshot verification of this revision is pending relaunch.
