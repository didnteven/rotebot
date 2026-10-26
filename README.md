# Macroscope

A desktop utility for turning a described repetitive task into a small, checked
tool. The first task family prepares a new CSV from a selected export. This is the
repository foundation: a Tauri 2/React shell, shared Rust contracts, bounded IPC,
and synthetic fixtures. Generation, transformation execution and connections are
not implemented yet.

Use stable Rust, Node 24.10.0 and npm, plus the native
[Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) for your OS.

```sh
npm ci
bash scripts/doctor.sh
bash scripts/check.sh
cd apps/desktop
npx tauri dev
```

For a browser preview, run `npm run -w apps/desktop dev` from the repository root;
it uses the in-memory backend. Build only the frontend with
`npm run -w apps/desktop build`. Compile the native binary with
`cd apps/desktop && npx tauri build --debug --no-bundle`.

`scripts/check.sh` checks formatting, Clippy, Rust tests, TypeScript, UI tests and
the Vite build. Keep the preset `CARGO_TARGET_DIR`. Cargo/npm lockfiles pin resolved
dependencies; key versions are recorded in [versions](docs/build/versions.md).

Each crate and desktop subdirectory owns its implementation so later tasks can
work independently. Contracts are generated from `macroscope-domain` with
`cargo run -p macroscope-domain --example export_schemas`; Rust tests reject schema
drift. Task/mapping JSON uses snake_case; tool manifests follow the plan's camelCase.
Ceilings and manifests describe intent; they do not grant access.

See the [master plan](plan/PLAN.md), [ADRs](docs/adr/), and
[owner overrides](docs/adr/0001-owner-overrides-2026-10-27.md).
