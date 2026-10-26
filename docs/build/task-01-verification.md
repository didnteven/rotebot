# Task 01 verification — 2026-10-27

All required commands passed with exit code 0 on macOS arm64, using the preset
`CARGO_TARGET_DIR`. Changes remain in the working tree; no commit, push or branch
change was made. Exact dependency pins are in [versions.md](versions.md).

| Command | Result |
| --- | --- |
| `npm install` (repository root) | PASS, exit 0; 0 vulnerabilities |
| `bash scripts/check.sh` | PASS, exit 0 |
| `cd apps/desktop && npx tauri build --debug --no-bundle` | PASS, exit 0; native binary compiled |

The check script passed formatting, Clippy with warnings denied, 25 Rust tests
(0 failures), TypeScript, 7 frontend tests in 2 files, and Vite production build.
The requested no-bundle build succeeded without a signing/bundling failure.

`npm ci` also passed from the root (exit 0, 0 vulnerabilities), verifying clean
installation from `package-lock.json`. `bash scripts/doctor.sh` passed (exit 0),
reporting Muse presence/version and the expected MSP fingerprint. Its only Muse
invocation was `muse --version`. No credentials or Keychain contents were accessed.

CI is configured for macOS and Windows desktop checks and Ubuntu pure crates.
Those hosted jobs have not been run; the results above are local macOS results.
The foundation has no CSV interpreter, model transport, MCP connection execution
or native vault implementation yet; its tests do not claim those later gates pass.

## Exact output tails

`npm install`:

```text

added 53 packages, and audited 170 packages in 495ms

33 packages are looking for funding
  run `npm fund` for details

found 0 vulnerabilities
```

`bash scripts/check.sh`:

```text


> @macroscope/desktop@0.1.0 test
> vitest --run


 RUN  v5.0.5 /Users/todd/StudioProjects/macroscope/apps/desktop


 Test Files  2 passed (2)
      Tests  7 passed (7)
   Start at  09:17:39
   Duration  914ms (environment 68%, setup 14%, tests 10%, import 6%, transform 2%)


> @macroscope/desktop@0.1.0 build
> vite build

vite v8.3.5 building client environment for production...
transforming...
✓ 21 modules transformed.
rendering chunks...
computing gzip size...
dist/index.html                   0.44 kB │ gzip:  0.28 kB
dist/assets/index-Dgnbc_8g.css    2.46 kB │ gzip:  1.00 kB
dist/assets/index-DIuriJVL.js   221.88 kB │ gzip: 69.53 kB

✓ built in 78ms
```

`cd apps/desktop && npx tauri build --debug --no-bundle`:

```text
   Compiling window-vibrancy v0.8.1
   Compiling tauri-runtime v2.12.2
   Compiling tauri-runtime-wry v2.12.3
   Compiling tauri v2.12.5
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 12.35s
       Built application at: /Volumes/250GB SAMSUNG 970 EVO PLUS /dev-tools/macroscope-cache/targets/01-foundation/debug/macroscope-desktop
```

Raw local logs are retained in `.dev/task-01/` (gitignored).
