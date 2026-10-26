# ADR 0001: Owner overrides to the master plan (2026-10-27)

Status: accepted. Source: owner instruction at the start of the build, 2026-10-27.

The master plan (`plan/PLAN.md`) stays authoritative except where this record changes it.
Each change keeps the plan's security requirements; it widens a route, not an authority.

## 1. Muse Code plan sign-in is a supported model-access route

**Plan said:** all Meta model use is billed to an app-owned API key; "do not require a
consumer login" (§0, §4.5).

**Owner now wants:** while the developer key is pending, the app can use Muse Code with the
owner's own Muse plan sign-in instead of direct API calls, and Settings offers a place to
either enter an API key or check the Muse plan sign-in.

**Decision:** Settings → Model access offers two routes, shown side by side:

| Route | How the credential reaches the model | Who signs in |
|---|---|---|
| Meta API key (recommended once available) | App-owned Keychain item → Bearer header (direct) or child-only `META_API_KEY` (Muse) | Key entered in a native secure field |
| Muse Code plan sign-in | Muse's own stored login, used by `muse exec` / `muse serve` started without `META_API_KEY` | The user, in their own terminal, with `muse login` |

Rules that keep the plan's credential boundary:

- The app **never** runs `muse login`, `muse logout`, `muse auth set`, or any MSP
  `account/*` method. It cannot change or clobber the owner's login (see the 2026-10-22
  incident in plan §4.5). "Sign in" in the UI shows the command and offers to open a
  Terminal window running `muse login` only when the user presses that button.
- Plan status comes from the read-only `muse usage --json` ("Show your subscription
  allowance (no model turn)"). Its output decides the state shown: signed in with an
  allowance, not signed in, Muse missing, or unknown.
- When an API key is stored, it takes priority (it is passed as `META_API_KEY`, which Muse
  documents as overriding the login). When no key is stored and the user selected the plan
  route, Muse runs with its normal home so it can find the login; the child environment is
  still allowlisted (no unrelated secrets), shell stays disabled, and the build workspace is
  synthetic. The plan route cannot use the direct Responses gateway, so generation goes
  through Muse only.
- The UI says which route each build used and that plan usage counts against the owner's
  Muse allowance, not API billing.

## 2. Connections (MCP) are in scope

**Plan said:** OAuth and network-enabled tools are out of the private MVP (§1.3).

**Owner now wants:** the app can set up connections such as a Gmail MCP server, and if a
recording has seen the user use Gmail, the app prompts the user to connect it (when it is
not already connected).

**Decision:**

- A **Connections** destination lists a catalog of connectors (Gmail, Google Calendar,
  Google Drive, Slack, Notion, GitHub, Linear, Jira, and a custom MCP server). Each entry
  is a remote MCP server (streamable HTTP) with an editable URL. Connecting runs the MCP
  authorization flow (OAuth 2.1 with PKCE, loopback redirect, dynamic client registration
  when the server offers it) in the system browser. Tokens live in app-owned Keychain
  items, never in the database, webview state, logs, or generated code.
- **Detection** turns observed app and site usage into a suggestion. It matches local
  signals (foreground app bundle id, window title, a browser host when available, and words
  in a Tell description) against each connector's signatures on the device. Only the
  connector id, the signal kind, and a count are stored; raw titles are discarded at the
  match. When a connector is detected and not connected, the app shows one dismissible
  prompt ("You used Gmail while recording. Connect Gmail so tools can use it.").
  Dismissal is remembered per connector. Detection only reads signals the user already
  allowed (an active Show recording, or text the user typed).
- Using a connection from a tool is a separate broker capability (`mcp.call`) that needs
  its own manifest entry, grant, and staged approval; it is not part of the first CSV
  family. Setting up and testing a connection (initialize + tools/list) works now.

## 3. Build in branches; no hosted repository yet

The public name "Macroscope" is taken, so no GitHub repository is created. Work is
committed on local branches that can be pushed and opened as pull requests later.
`docs/build/BRANCHES.md` lists the branches, their bases, and the order to open them.

## 4. Tooling deviations

- npm workspaces instead of pnpm: pnpm is not installed on the build machine and the
  coding agents run without global installs. The workspace layout is the same.
- Coding is done mainly by Codex (`gpt-6.1-sol`), reviewed and tested by Claude, with
  Muse Code (`muse-spark-1.3`, contributor mode on) as a helper whenever the owner's Muse
  sign-in is available. Contributor mode is a development-time choice for building this
  repository only; the shipped app still never enables contributor mode (plan §8.3 P5).
