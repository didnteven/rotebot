# API discovery verification — 2026-10-31

Implementation: [ADR 0006](../adr/0006-api-discovery.md).

Git base: `f14263c` on `m4/custom-endpoint`, with pre-existing working-tree changes
preserved. The requested pull was attempted, but this checkout has no remote or
tracking branch. No repository URL was supplied during implementation. Verification
preceded the API commit; no pushes, deployments or real service connections were made.

Commit scope: the API commit excludes the pre-existing tool-editing/version-history
and custom-model-setting changes. An isolated API-only tree passes the core compile
check and focused API UI tests. Both that tree and its unchanged base have the same
two TypeScript errors in `App.tsx`: the already-committed parent references an
uncommitted `ToolVersions` component and passes `editTool` to a child whose matching
changes are still uncommitted. The full-working-tree typecheck/build results below
include those local edits; the API commit does not silently publish them.

## Checks

| Check | Result |
| --- | --- |
| Frontend typecheck | Passed |
| Full frontend suite | 198 passed in 28 files, including 9 API access UI tests |
| API integration suite with synthetic worker | 14 passed |
| Connection integration regression suite | 14 passed |
| Model-source regression suite | 18 passed |
| Web-source regression suite | 20 passed |
| DTO serialization suite | 8 passed |
| Gateway response regression suite | 6 passed |
| Connection unit tests | 23 passed |
| Builder unit tests | 10 passed |
| Frontend production build | Passed |
| Native desktop compilation | Passed |
| Changed Rust files formatting / diff whitespace | Passed |

API integration uses real host/gateway/transport code with local HTTP stand-ins
and synthetic credentials. It covers known connection reuse, supported connection
setup before a custom API, public API reuse, bounded hosted search, documentation
evidence, unsupported OAuth, stale review, private-network denial, all three
authentication modes, key non-disclosure, redirect refusal, pagination indicators,
oversize responses, shared rate limits, stop/revocation and disconnect. A worker
integration test builds, saves and stages an API-backed tool through the existing
grant path, cancels an in-flight staged read and verifies disconnect prevents
another request. A separate regression verifies HTTP 401/403/429 stop web builds
after one model call and one source attempt, without repair/source substitution.

The worker test deliberately uses the synthetic test mode, not new OS sandbox
admission evidence. No live-provider/model discovery evaluation or visual browser
verification was performed; Browser discovery returned no available browser.
The supported slice remains one complete JSON GET, with no OAuth, POST adapter,
automatic pagination or external API writes.

## Commands

```sh
npm run -w apps/desktop typecheck
npm run -w apps/desktop test -- --run
npm run -w apps/desktop build

cargo test --offline -p macroscope-app-core --features synthetic-runtime-tests \
  --test api_access --test connections --test model_sources --test web_sources --test dto_shapes
cargo test --offline -p macroscope-model-gateway --test responses
cargo test --offline -p macroscope-connections --lib
cargo test --offline -p macroscope-builder --lib

MACROSCOPE_SWIFT_SCRATCH=/private/tmp/macroscope-api-discovery-swift \
MACROSCOPE_SWIFT_DISABLE_SANDBOX=1 CARGO_NET_OFFLINE=true \
  npm run -w apps/desktop tauri -- build --debug --no-bundle
```

This environment required `/Users/todd/.cargo/bin` on PATH and execution permission
for local test-server listeners. The native build's capture-helper script defaults
to an external-drive scratch directory that was not writable here; the temporary
scratch override above avoids that dependency. Swift package sandboxing was disabled
for this local helper compilation inside the enclosing workspace sandbox.

Strict library Clippy was attempted. Existing warnings in `connections/src/mail.rs`
(`unnecessary_sort_by`) and recording masking (`single_range_in_vec_init` and
`manual_range_contains`) prevent the unrestricted `-D warnings` check. Checking
the affected libraries with just those three inherited lint categories allowed
passes:

```sh
cargo clippy --offline -p macroscope-app-core -p macroscope-connections \
  -p macroscope-model-gateway --lib -- -D warnings \
  -A clippy::unnecessary-sort-by -A clippy::single-range-in-vec-init \
  -A clippy::manual-range-contains
```

Whole-workspace formatting also reports pre-existing differences in unrelated
recording/examples/mail files. Only files changed for this implementation were
formatted; unrelated working-tree edits were not normalized.
