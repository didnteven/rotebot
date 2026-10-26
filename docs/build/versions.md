# Foundation dependency pins

Chosen on 2026-10-27 from the package registries. Tauri stays on stable 2.x.
JavaScript dependencies use exact versions in `apps/desktop/package.json`;
`package-lock.json` and `Cargo.lock` pin transitive resolutions.

| Package | Exact version |
| --- | --- |
| `tauri` (Rust) | 2.12.5 |
| `tauri-build` (Rust) | 2.7.4 |
| `@tauri-apps/cli` | 2.12.4 |
| `@tauri-apps/api` | 2.12.4 |
| `react` / `react-dom` | 19.3.1 |
| `vite` | 8.3.5 |
| `@vitejs/plugin-react` | 6.1.2 |
| `typescript` | 7.0.2 |
| `vitest` | 5.0.5 |
| `jsdom` | 29.1.1 |
| `@testing-library/react` | 16.3.3 |
| `@testing-library/jest-dom` | 7.0.1 |
| `@testing-library/user-event` | 14.6.8 |

Build environment: macOS arm64, Node 24.10.0, npm 11.6.0, cargo 1.99.1,
rustc 1.99.1. `rust-toolchain.toml` intentionally tracks `stable` as requested;
it is not a frozen compiler pin. CI uses Node 24.10.0 and stable Rust.

The two app commands are enrolled in the Tauri application manifest and granted
only to `main`, following the official
[capability documentation](https://v2.tauri.app/security/capabilities/).
No updater, plugins, tray, model transport or native secret access is enabled.

jsdom 29.1.1 supports Node 24.10.0; jsdom 30 requires a newer Node patch.
It is also declared at the workspace root so npm-hoisted Vitest can resolve its
optional environment peer consistently after both install and clean install.
