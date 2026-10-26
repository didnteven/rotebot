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

To publish later:

```sh
git remote add origin <url>
git push -u origin main m0/foundation m0/runtime-gate m1/csv-broker m0/model-access \
  m2/connections m5/observer-experiment m2/desktop-ui m2/app-integration m3/hardening
```

Then open each PR with the base from the table. `docs/build/PROGRESS.md` records what each
branch passed, failed, or left unverified.
