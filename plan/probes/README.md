# MSP probes (track 04)

- `msp_probe.py`: a minimal MSP client. It spawns `muse serve` and speaks newline-delimited JSON-RPC 2.0 over stdio. It writes every raw frame to a JSONL transcript (`dir`: `c2s`, `s2c`, `stderr`, `meta`, `note`). Scenarios: `basic`, `badkey`, `features`, `fork`, `crash`, `account`, `sandbox`, `worktree`, `framing-negative`.
- `summarize.py <transcript>`: prints a readable timeline of a transcript.

Every run used the free `echo` provider or a **bogus** key. No paid model call was made.

| Transcript | What it shows |
|---|---|
| 01-echo-basic | handshake, session/start, one streamed turn (echo, ephemeral) |
| 02-meta-badkey-serve | `META_API_KEY=sk-invalid-test-key`, real muse home: `turn/completed` failed, `authRequired` |
| 03-meta-nokey-isolated-serve | no key, isolated XDG home: logged-out fallback, `authRequired` "not logged in" |
| 04/06 exec-apikeystdin-badkey | `muse exec --api-key-stdin`, isolated home: exit 1, stderr only, no JSONL |
| 05 exec-envkey-badkey | same with the env var: identical |
| 07 exec-apikeystdin-badkey-realhome | `--api-key-stdin`, real home: JSONL `run.terminal.failed`, exit 1 |
| 08-meta-badkey-isolated-serve | bad env key, isolated home: catalog rejected, logged-out fallback, `authRequired` |
| 09/10 features | userShell, goal/set, skill/plugin/hook/task list, fork; ephemeral vs durable |
| 11/12 fork-resume | fork (cut vs no cut), rename, resume after a host restart |
| 13-meta-account-experimental | experimental `account/*` (**side effect: clobbered the global Keychain credential, see 04-muse-msp.md**) |
| 14 crash-midturn | SIGKILL the host mid-turn, then resume: orphaned turn reconciled as `cancelled` |
| 14a serve-bad-config-key | unknown `-c` key: host refuses to start, stderr only |
| 15 framing-negative | pre-init request, bad JSON, double initialize, unknown method |
| 16 sandbox-* | userShell under default / `--sandbox-network restricted` / `--disable-shell` |
| 17 fork-worktree | fork plus `turn/start.workspaceRoots`: accepted, but the shell cwd did not move |
| 18 env-leak-usershell | `META_API_KEY` from the host env is visible to userShell commands |
| 19 serve-baseurl-config | `muse serve` rejects `--base-url` (exec-only flag) |
| 19 fake-endpoint-requests / 20 exec-baseurl-fake | `muse exec --base-url` against `fake_meta_endpoint.py`: `GET /muse-code/models`, `Authorization: Bearer <key>`, `x-client-id` |

`fake_meta_endpoint.py` is a local stand-in that always answers 401. It logs the request path, header names and the masked auth scheme.
