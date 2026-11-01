# Nondeveloper navigation and readability review

Reviewed all desktop UI flows on the current local tip (`65561aa`) with the in-progress bot theme. This review uses source inspection and interaction tests. A native visual walkthrough and sessions with nontechnical users remain necessary before claiming usability has been validated.

## Changes made

| Flow | Problem | Change |
| --- | --- | --- |
| Main navigation | Section names alone do not explain what belongs where. | Each section has a short purpose statement and optional three-step guidance. Keyboard navigation keeps its existing predictable tab order. |
| Leaving a task to change settings or connect a service | Users can lose their sense of where the unfinished task went. | A Return to your task banner brings them back to the still-mounted flow, preserving their description. |
| Create/edit | A blank request field lacks guidance; Back can mean several things across steps. | Added a concrete plain-language example, a request placeholder, and a separate Back to My tools exit. Existing three-step progress and review boundaries remain. |
| Run | Initial loading errors can leave an unusable screen and rejected promises. Saving terminology lacks context. | Explicit tool loading and retry recovery, stale-response protection, and a plain explanation that saving creates a new file and keeps the original. |
| Today | Empty state offers little help to someone who already has a tool. | Explains where daily results appear and how to reach Schedule this tool from My tools. |
| Activity | Users may confuse activity history with results awaiting approval. | Section guidance distinguishes history/receipts from Today and explains how to locate output files. |
| Connections/custom APIs | CSV and connection concepts are unfamiliar. | Defines CSV in everyday terms, explains when connections are unnecessary, and retains separate loading/empty states and sign-in feedback. Custom API setup continues to show secure-key and test steps. |
| Model/settings | Model routes, credentials, and custom endpoints can look equally required. | Explains Ready and selection, marks custom-provider setup optional, adds a plain description of what provider details mean, and explains where schedules live. Focus reaches model selection without forcing a scroll past the introduction. |
| Schedules | Existing saved choices are not loaded into the editor; time zone is unstated; invalid monthly days lack clear feedback. | Loads saved frequency/time/destinations, identifies the local time zone, explains date-based filenames, provides specific missing-step messages, and validates time/day/destination before approval. |
| Recording | Long flow lacks an overall progress map; Back silently discards the recording. | Five-stage progress, explicit discard/back explanation, contextual bot feedback, and immediate permission-status updates. Privacy review and Send consent remain required. |
| Privacy | Deletion dominates the start of the screen; Emergency stop is disabled while ordinary history loads. | Cloud controls and stop actions precede deletion. Emergency stop has an independent action state and remains usable during a refresh. Data-retention and deletion consequences stay visible. |
| Version history | Restoring a version may not make its effect clear. | Explains that it changes future runs and may require schedule approval again. |
| Typography/resizing | Inconsistent reading rhythm and font sizes; appearance picker disappears in small windows. | Platform-native system UI stack (SF Pro on macOS), consistent heading/body rhythm, readable line lengths, monospace reserved for code, numeric alignment for times/badges, and a visible narrow-window theme selector. No remote font dependency. |

## Validation

Interaction coverage includes draft preservation across Settings, saved schedule hydration and invalid-day blocking, retry after initial tool-load failure, Strict Mode loading, and Emergency stop during an unresolved history request. Existing tests cover keyboard navigation, source/action approval, recording permissions, editing/versioning, model configuration, privacy controls, and hostile text rendering.

## Follow-up validation

- Walk through the native app at normal and narrow window sizes in both themes, with larger text/zoom and reduced motion.
- Ask nontechnical users to create a task, connect a service, schedule it, locate its result, recover from a failed request, and stop ongoing work without instructions.
- Measure where users hesitate before deciding whether to rename product-wide concepts such as tool, model access, API, and receipt. Do not simplify away exact service access or action approval information.
