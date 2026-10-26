Track: WAYS TO BUILD SOFTWARE INSIDE THE APP (alternatives and fallbacks). Output: plan/research/05-alt-builders.md
The owner wants Muse Code from a terminal inside the app, "or another way to build software/scripts inside of software". Compare concretely (use web search):
1. Muse Code via `muse serve` (MSP over stdio) with a native UI. (The schema is in plan/reference/msp/msp.d.ts; skim it.)
2. Muse Code TUI in an embedded PTY terminal (xterm.js / SwiftTerm / portable-pty).
3. `muse exec --json` one-shot headless jobs.
4. Our own small agent loop calling the Meta Model API directly with a constrained tool set (write file, run test, read UI tree), with no general shell.
5. Other agents as optional backends: `codex app-server`, Claude Agent SDK. Explain the value of a provider-agnostic "Builder" interface even if Meta is the default.
6. Non-agent generation: a constrained DSL / workflow graph (like Shortcuts) that the model fills in, which is far safer and easier to verify than arbitrary code.
Recommend a layered answer: likely, generated automations target a small, typed Macroscope SDK/DSL first (safe, inspectable, repairable), with full code via Muse for the cases that need it. Define that SDK surface: primitives (read clipboard, file ops, HTTP, browser actions via Playwright/CDP, AX UI actions, spreadsheets, notifications, LLM call), triggers, parameters, and how each primitive maps to permissions. Show 3 example automations written against it.
