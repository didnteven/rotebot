import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it, vi } from "vitest";
import { App } from "./App";
import { MockBackend } from "./backend/mock";
import { BackendError, toBackendError } from "./backend/tauri";
import catalog from "../../../contracts/connectors.json";

it("mock connector DTOs match every catalog field exposed by the core", async () => {
  const actual = await new MockBackend().listConnectors();
  expect(actual).toEqual(catalog.connectors.map((c) => ({
    id: c.id, name: c.name, category: c.category, mcpUrl: c.mcp_url,
    registration: c.auth.registration, verification: c.verification,
    docsUrl: c.auth.docs_url || null, clientSetup: c.auth.client_setup ?? null,
    redirectUri: c.auth.fixed_redirect_port == null ? null : `http://127.0.0.1:${c.auth.fixed_redirect_port}/callback`,
    state: { kind: "not_connected" }, hasClientCredentials: false, service: (c as { service?: string }).service ?? c.id,
    mail: c.mail ? { help: c.mail.app_password_help, customServer: !!c.mail.custom_server, imapHost: c.mail.imap_host, imapPort: c.mail.imap_port,
      smtpHost: c.mail.smtp_host, smtpPort: c.mail.smtp_port, smtpStarttls: !!c.mail.smtp_starttls, email: null,
      appPasswordUrl: (c.mail as { app_password_url?: string }).app_password_url ?? null } : null,
  })));
});

it("Muse containment opt-in defaults off, persists, and can be revoked", async () => {
  const backend = new MockBackend();
  const status = await backend.modelAccess();
  vi.spyOn(backend, "modelAccess").mockResolvedValueOnce({ ...status, muse: { kind: "signed_in", version: "test", planSummary: null } });
  const save = backend.setMuseUnverifiedOk.bind(backend);
  const set = vi.spyOn(backend, "setMuseUnverifiedOk").mockImplementation(async (ok) => ({
    ...await save(ok), muse: { kind: "signed_in", version: "test", planSummary: null },
  }));
  const user = userEvent.setup();
  render(<App backend={backend} />);
  await screen.findByRole("button", { name: "Try a sample" });
  await user.click(screen.getByRole("button", { name: "Settings" }));
  const label = "Allow Muse builds before file-access confinement is verified";
  expect(await screen.findByRole("checkbox", { name: label })).not.toBeChecked();
  expect(screen.getByText("Muse runs with its shell and file writes turned off in a separate build folder, but its file-reading tools have not yet been proven to stay inside that folder.")).toBeVisible();
  await user.click(screen.getByRole("checkbox", { name: label }));
  await waitFor(() => expect(set).toHaveBeenCalledWith(true));
  expect((await backend.modelAccess()).museUnverifiedOk).toBe(true);
  await waitFor(() => expect(screen.getByRole("checkbox", { name: label })).toBeChecked());
  await user.click(screen.getByRole("checkbox", { name: label }));
  await waitFor(() => expect(set).toHaveBeenLastCalledWith(false));
  expect((await backend.modelAccess()).museUnverifiedOk).toBe(false);
});

it("Tools loads and displays persisted connection suggestions", async () => {
  const backend = new MockBackend();
  const load = vi.spyOn(backend, "connectionSuggestions").mockResolvedValue([
    { connectorId: "gmail", connectorName: "Gmail", message: "Connect Gmail from your last recording.", signalKind: "window_title", count: 1 },
  ]);
  render(<App backend={backend} />);
  expect(await screen.findByText("Connect Gmail from your last recording.")).toBeVisible();
  expect(load).toHaveBeenCalled();
});

it("connection Cancel disconnects the pending connector and failed connect shows the backend message", async () => {
  const backend = new MockBackend();
  vi.spyOn(backend, "completeConnect").mockImplementationOnce(() => new Promise(() => {}));
  const disconnect = vi.spyOn(backend, "disconnect");
  const user = userEvent.setup();
  render(<App backend={backend} />);
  await screen.findByRole("button", { name: "Try a sample" });
  await user.click(screen.getByRole("button", { name: "Connections" }));
  await user.click(screen.getByRole("button", { name: "Connect Notion" }));
  let details = within(screen.getByRole("region", { name: "Notion connection details" }));
  await details.findByText("Finish signing in in your browser…");
  await user.click(details.getByRole("button", { name: "Cancel" }));
  await waitFor(() => expect(disconnect).toHaveBeenCalledWith("notion"));
  // Switch detail to release the deliberately unresolved UI call.
  vi.spyOn(backend, "beginConnect").mockRejectedValueOnce(new BackendError("OAuth client is missing.", "invalid"));
  await user.click(screen.getByRole("button", { name: "Connect Linear" }));
  details = within(screen.getByRole("region", { name: "Linear connection details" }));
  expect(await details.findByRole("alert")).toHaveTextContent("OAuth client is missing.");
});

it("malformed rejection objects never stringify arbitrary payloads as errors", () => {
  for (const message of [{ token: "SECRET" }, ["SECRET"], null, 42]) {
    expect(toBackendError({ message, code: "bad" }).message).toBe("Something went wrong. Try again.");
  }
});

it("successful connections show the tool count reported by the backend", async () => {
  const backend = new MockBackend();
  const connector = (await backend.listConnectors()).find((c) => c.id === "notion")!;
  vi.spyOn(backend, "completeConnect").mockResolvedValue({ ...connector, state: { kind: "connected", sinceMs: 1, toolCount: 2 } });
  const user = userEvent.setup();
  render(<App backend={backend} />);
  await screen.findByRole("button", { name: "Try a sample" });
  await user.click(screen.getByRole("button", { name: "Connections" }));
  await user.click(screen.getByRole("button", { name: "Connect Notion" }));
  expect(await screen.findByText("Connected · 2 tools available")).toBeVisible();
});
