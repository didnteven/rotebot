# Macroscope (working name)

A macOS-first desktop utility that turns a described repetitive task into a small,
checked tool. The first task family prepares a new CSV from a selected export: you
describe the job, confirm the rules, review what leaves the Mac, and Macroscope builds
a finite mapping that is checked on your own file before you save it. Every run stages
the output, shows it to you, and only then creates the new file (never overwriting).

It also suggests **connections** (Gmail, Slack, Notion, …) when you mention a service in a
task or use it during a **Show** recording, and connects them over MCP with OAuth.

## Run it

Prerequisites: stable Rust, Node 24 with npm, Xcode command-line tools (macOS).

```sh
npm ci
(cd native/macos-observer && swift build -c release)   # enables Show
bash scripts/doctor.sh
cd apps/desktop && npx tauri dev
```

A double-clickable app for this Mac (unsigned, observer included): `scripts/bundle-mac.sh`
(add `--release` for an optimized build). It prints the path to `Macroscope.app`.

Browser preview with the in-memory mock backend: `npm run -w apps/desktop dev`.
Native binary: `cd apps/desktop && npx tauri build --debug --no-bundle`.
All checks (fmt, Clippy, Rust tests, TypeScript, UI tests, Vite build): `bash scripts/check.sh`.

### Model access (Settings → Model access)

| Route | What you need | Notes |
|---|---|---|
| Meta API key | A Meta developer API key | Entered in a native hidden-input dialog, stored in your Keychain, billed to the key |
| Muse plan | `muse login` in Terminal (Macroscope never signs in for you) | Uses your Muse allowance; needs an explicit opt-in until Muse's file-access confinement is verified |
| Offline demo | Nothing | Builds from your confirmed rules without a model; labelled as a demo |

## What is where

| Path | Purpose |
|---|---|
| `crates/app-core` | The product service behind the UI contract: drafts, builds, host checks, tools, runs, Show |
| `crates/executor`, `crates/effect-broker` | Finite CSV mapping engine; stage → approve → atomic no-overwrite commit |
| `crates/model-gateway`, `crates/muse-client`, `crates/builder` | Meta Responses client, Muse supervisor/MSP client, mapping builder |
| `crates/connections` | Connector catalog, MCP client, OAuth (PKCE, DCR), detection |
| `crates/storage`, `crates/vault`, `crates/policy` | SQLCipher database, Keychain, cloud-egress epochs |
| `apps/desktop` | Tauri shell (`src-tauri`) and React UI (`src`) with the typed `Backend` contract |
| `native/macos-observer` | Swift metadata-only observer for Show |
| `experiments/runtime-gate` | Deno + Seatbelt admission experiment (verdict: not admitted) |
| `docs/build/PROGRESS.md` | What passed, failed, or is still unverified |

See the [master plan](plan/PLAN.md), the [ADRs](docs/adr/) (owner overrides in
[0001](docs/adr/0001-owner-overrides-2026-10-27.md)), and [branch map](docs/build/BRANCHES.md).
