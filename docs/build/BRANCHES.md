# Build branches

No hosted repository exists yet (ADR 0001 §3). Each branch below is meant to become one pull
request. Open them in this order, each against the base listed, so every PR shows one step of
the build.

| Order | Branch | Base | What it adds | Plan milestone |
|---|---|---|---|---|
| 1 | `m0/foundation` | `main` | Workspace, crate skeletons, domain types, NDJSON framing, Tauri/React shell, fixtures, checks, CI | M0 day 1 |
| 2 | `m0/runtime-gate` | `m0/foundation` | Pinned Deno under a Seatbelt profile, canaries, runtime lock evidence | M0 day 6 |
| 3 | `m1/csv-broker` | `m0/foundation` | CSV mapping engine, encrypted staging, plan hash, atomic no-overwrite commit, journal | M0 day 7, M1 |
| 4 | `m0/model-access` | `m0/foundation` | Vault, egress epochs, Meta Responses gateway, Muse supervisor + MSP client, plan sign-in check, mapping builder | M0 days 2–5, 8 |
| 5 | `m2/connections` | `m0/foundation` | Connector catalog, MCP client, OAuth (PKCE, DCR), detection ledger | ADR 0001 §2 |
| 6 | `m5/observer-experiment` | `m0/foundation` | Swift metadata-only observer with on-device connector matching | M0 day 9 / M5 |
| 7 | `m2/desktop-ui` | `m0/foundation` | React UI for every screen against the typed contract, mock backend, review fixes | M2 |
| 8 | `m2/app-integration` | merge of 2–7 | Encrypted storage, app core, Tauri commands, Show controller, connections wiring, docs | M1–M2 |
| 9 | `m3/hardening` | `m2/app-integration` | Independent Codex review of the Claude-written core with fixes; remaining UI items | M3 |
| 10 | `m3/privacy-controls` | `m3/hardening` | Privacy center data deletion, tool export/pause, Try another sample, tray tool rows | M2–M3 |
| 11 | `m3/renderer-safety` | `m3/privacy-controls` | P11: hostile text on every surface renders as text, pinned CSP; startup failure dialog; Muse plan route made to work for real | M3 |
| 12 | `m3/muse-findings` | `m3/renderer-safety` | Fixes for the 8 defects Muse Code found in its review | M3 |
| 13 | `m4/runtime-v2` | `m3/muse-findings` | QuickJS runner in a Seatbelt-confined child: generated-code boundary passes developer admission | M4 |
| 14 | `m4/dev-file-vault` | `m4/runtime-v2` | Debug builds keep secrets in a private 0600 file (no Keychain prompts); release builds use the Keychain | M4 |
| 15 | `m4/general-tasks` | `m4/dev-file-vault` | Tasks beyond CSV: model-written programs in the sandbox, checked, previewed, approved, committed; ADR 0004 | M4 |
| 16 | `m4/connection-actions` | `m4/general-tasks` | Programs plan connection calls; the host runs only approved calls, with journal and receipts, never resending after a crash | M4 / ADR 0004 |
| 17 | `m4/web-sources` | `m4/connection-actions` | Web sources ("check the weather"), no-input tasks (built by Muse Code as a trial); example-format check with one correction round; merges connection actions | M4 |
| 18 | `m4/schedules` | `m4/web-sources` | Approved schedules, encrypted Today digest, held connection actions, macOS notifications and opt-in launch at login | M4 / ADR 0004 §4–5 |
| 19 | `m4/model-step` | `m4/schedules` | Approved connection reads, bounded model replay, monthly caps, outbound review and scheduled calendar/bills summaries | ADR 0004 §1, §3 |

To publish later:

```sh
git remote add origin <url>
git push -u origin main m0/foundation m0/runtime-gate m1/csv-broker m0/model-access \
  m2/connections m5/observer-experiment m2/desktop-ui m2/app-integration m3/hardening m3/privacy-controls m3/renderer-safety m3/muse-findings m4/runtime-v2 m4/dev-file-vault m4/general-tasks m4/connection-actions m4/web-sources m4/schedules m4/model-step
```

Then open each PR with the base from the table. `docs/build/PROGRESS.md` records what each
branch passed, failed, or left unverified.
