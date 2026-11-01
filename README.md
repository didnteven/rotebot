# Rotebot

**Show a chore once, get a small checked tool.** Rotebot is a macOS desktop app that
turns a repetitive task into a tool you can rerun. Describe the job in plain language, or
**Show** it: pick a window, do the chore once, and Rotebot works out what you did. Either
way you review exactly what would leave your Mac, then Rotebot builds a small program that
reads the data you chose (a file, a web page or public API, or a connected service such as
Gmail), is checked against the real data, and produces a new file or a set of planned
actions for you to approve. Nothing is created or sent until you approve it, and files are
never overwritten.

Most people can't easily describe an automation, and an AI agent that acts on your files
and accounts unchecked is hard to trust. Rotebot keeps the model out of the run loop: it
writes a finite program once, the host checks it, and the program runs in a sandbox.

## How it works

1. **Describe or Show.** Type what you want, or record yourself doing it once. Show
   masks private details in the captured frames and asks about anything ambiguous.
2. **Review what leaves the Mac.** A screen lists the data and instruction that would be
   sent to the model, and nothing goes out before you confirm it.
3. **Build and check.** The model writes a program. The host runs it against the real
   data and checks the result, for example the output format or an example you gave. If a
   check fails, the model gets the difference and tries once more.
4. **Approve and run.** The output is staged for your review, then written as a new file.
   A tool that sends or changes something stages the planned actions instead, and they
   wait in a "Needs you" queue for approval. Tools can run on a schedule while the app is
   open.

It also suggests **connections** (Gmail, Slack, Notion, …) when you mention a service in a
task or use it during a Show recording, and connects them over MCP with OAuth.

It also suggests **connections** (Gmail, Slack, Notion, …) when you mention a service in a
task or use it during a **Show** recording, and connects them over MCP with OAuth.

Program plans offer **Send and check access** to reuse suitable connections and known
public APIs, then research missing API access and show setup instructions. Custom
API connections support one approved JSON GET with a native-entered bearer token
or API key header. See [API discovery](docs/adr/0006-api-discovery.md) for supported
authentication, research providers and limits.

## How Rotebot uses Meta's AI tooling

| Capability | Where it is used |
|---|---|
| Responses API, `muse-spark-1.3` | Writes the checked programs (`crates/model-gateway`, `crates/builder`) |
| Structured output (`json_schema`) | Models return programs the host can validate before anything runs |
| Vision (`input_image`) | Show sends the masked recording frames so the model can work out what you did (`crates/app-core/src/recording`) |
| Built-in `web_search` tool | Researches a service's API when you mention one in a task |
| Image generation and edits, `muse-image-1.0` | Creates each tool's bot character, using a reference image to keep the style consistent |
| Muse plan route | Runs through the user's own Muse allowance via a custom MSP client (`crates/muse-client`), as an alternative to a Meta API key |

## How it was built

Rotebot was built over several days (the full commit history is in this repo) with AI
coding assistants: Claude Code and Codex wrote and reviewed most of the code. Muse Code
(`muse-spark-1.3`) was used as an independent, read-only reviewer. Its two reviews
([first](docs/build/muse-review.md), [second](docs/build/muse-review-2.md)) found 16
defects, including OAuth client credentials that could reach a different server, and API
keys in web-source URLs that were being logged. Each was reproduced with a failing test
and then fixed. The working name during development was *Macroscope*; internal crate and
binary names and the `MACROSCOPE_*` environment variables still use it.

## Run it

Prerequisites: stable Rust, Node 24 with npm, Xcode command-line tools (macOS).

```sh
npm ci
(cd native/macos-observer && swift build -c release)   # enables Show
bash scripts/doctor.sh
cd apps/desktop && npx tauri dev
```

A double-clickable app for this Mac (unsigned, observer included): `scripts/bundle-mac.sh`
(add `--release` for an optimized build). It prints the path to `Rotebot.app`.

Browser preview with the in-memory mock backend: `npm run -w apps/desktop dev`.
Native binary: `cd apps/desktop && npx tauri build --debug --no-bundle`.
Debug builds keep secrets in `dev-secrets/secrets.json` (0600, in a 0700 directory) in Rotebot's data folder to avoid Keychain prompts; macOS release builds use the Keychain, with `MACROSCOPE_VAULT=file|keychain` available to test either store.
All checks (fmt, Clippy, Rust tests, TypeScript, UI tests, Vite build): `bash scripts/check.sh`.

### Model access (Settings → Model access)

| Route | What you need | Notes |
|---|---|---|
| Meta API key | A Meta developer API key | Entered in a native hidden-input dialog, stored in the app's secret store, billed to the key |
| Muse plan | `muse login` in Terminal (Rotebot never signs in for you) | Uses your Muse allowance; needs an explicit opt-in until Muse's file-access confinement is verified |
| Offline demo | Nothing | Builds from your confirmed rules without a model; labelled as a demo |

## What is where

| Path | Purpose |
|---|---|
| `crates/app-core` | The product service behind the UI contract: drafts, builds, host checks, tools, runs, Show |
| `crates/executor`, `crates/effect-broker` | Deterministic CSV interpreter, and the stage → approve → create-only commit broker |
| `crates/model-gateway`, `crates/muse-client`, `crates/builder` | Meta Responses client, Muse supervisor/MSP client, mapping builder |
| `crates/connections` | Connector catalog, MCP client, OAuth (PKCE, DCR), detection |
| `crates/storage`, `crates/vault`, `crates/policy` | SQLCipher database, Keychain, cloud-egress epochs |
| `apps/desktop` | Tauri shell (`src-tauri`) and React UI (`src`) with the typed `Backend` contract |
| `native/macos-observer` | Swift metadata-only observer for Show |
| `experiments/runtime-gate` | Deno + Seatbelt admission experiment (verdict: not admitted) |
| `docs/build/PROGRESS.md` | What passed, failed, or is still unverified |

See the [master plan](plan/PLAN.md), the [ADRs](docs/adr/) (owner overrides in
[0001](docs/adr/0001-owner-overrides-2026-10-27.md)), and [branch map](docs/build/BRANCHES.md).
