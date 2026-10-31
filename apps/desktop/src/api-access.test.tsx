import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, expect, test, vi } from "vitest";
import { ApiAccessPanel, ApiSetup } from "./ApiAccess";
import { MockBackend } from "./backend/mock";
import type { AccessReport, ApiProposal, TaskDraft } from "./backend/types";

afterEach(cleanup);
const proposal: ApiProposal = { name: "Example records", url: "https://api.example.com/records", auth: "bearer", headerName: "", docsUrl: "https://docs.example.com/api",
  method: "GET", setupInstructions: ["Create a token with record read access."], prerequisites: ["Access to this account"], coverage: "Complete records" };
function report(state: string, api: ApiProposal | null = null): AccessReport { return { state, api, connectorId: null, summary: "Access check result", sources: [], searched: false, checkedAtMs: 1, binding: "test" }; }
async function fixture(plan?: AccessReport) {
  const backend = new MockBackend(); await backend.saveApiKey("mk-SYNTHETIC-MODEL-KEY-0001"); await backend.selectRoute("api_key");
  const draft = await backend.draftTaskNoInput("Read my records"); draft.accessPlan = plan;
  return { backend, draft, status: await backend.modelAccess() };
}
async function panel(plan?: AccessReport) {
  const f = await fixture(plan); const accept = vi.fn(); const onBusy = vi.fn();
  const view = render(<ApiAccessPanel {...f} accept={accept} onBusy={onBusy} />);
  await waitFor(() => expect(screen.getByRole("button", { name: /Send and check access|Check access again/ })).toBeEnabled());
  return { ...f, accept, onBusy, ...view };
}

test("preparing the access review is local; only its explicit action starts research", async () => {
  const f = await fixture(); const research = vi.spyOn(f.backend, "researchTaskAccess");
  render(<ApiAccessPanel {...f} accept={vi.fn()} onBusy={vi.fn()} />);
  const button = screen.getByRole("button", { name: "Send and check access" });
  await waitFor(() => expect(button).toBeEnabled()); expect(research).not.toHaveBeenCalled();
  await userEvent.click(button);
  await waitFor(() => expect(research).toHaveBeenCalledWith(f.draft.draftId, expect.any(String)));
});

test("a known public API needs no connection or token setup box", async () => {
  await panel(report("public_api", { ...proposal, auth: "none" }));
  expect(screen.getByText("Public API found")).toBeVisible();
  expect(screen.queryByRole("region", { name: "Custom API setup" })).not.toBeInTheDocument();
  expect(screen.queryByRole("button", { name: "Save API connection" })).not.toBeInTheDocument();
});

test("a suitable connected API is selected while preserving other selected services", async () => {
  const f = await fixture();
  const connection = await f.backend.saveApiConnection({ name: "Example", url: proposal.url, auth: "none", headerName: "", docsUrl: proposal.docsUrl });
  await f.backend.testConnection(connection.id);
  f.draft.accessPlan = { ...report("existing_connection"), connectorId: connection.id };
  const accept = vi.fn(); const select = vi.spyOn(f.backend, "selectTaskConnections");
  render(<ApiAccessPanel {...f} accept={accept} onBusy={vi.fn()} />);
  await userEvent.click(screen.getByRole("button", { name: "Use this existing connection" }));
  await waitFor(() => expect(select).toHaveBeenCalledWith(f.draft.draftId, [connection.id]));
  expect(accept).toHaveBeenCalledWith(expect.objectContaining({ requiredConnections: [expect.objectContaining({ connectorId: connection.id })] }));
  expect(screen.queryByRole("button", { name: "Save API connection" })).not.toBeInTheDocument();
});

test("a supported service offers its existing connection setup without a custom API box", async () => {
  const f=await fixture({...report("connect_service"),connectorId:"gmail"}); const connect=vi.fn();
  render(<ApiAccessPanel {...f} accept={vi.fn()} onBusy={vi.fn()} connect={connect} />);
  await userEvent.click(screen.getByRole("button",{name:"Set up this available connection"}));
  expect(connect).toHaveBeenCalledWith("gmail");
  expect(screen.queryByRole("button",{name:"Save API connection"})).not.toBeInTheDocument();
});

test("OAuth and POST proposals explain the missing adapter without a credential action", async () => {
  await panel(report("needs_adapter", { ...proposal, auth: "oauth", method: "POST" }));
  expect(screen.getByText(/needs an adapter for its authentication/)).toBeVisible();
  expect(screen.queryByRole("button", { name: "Save API connection" })).not.toBeInTheDocument();
});

test("API credentials never enter the DOM and saving does not issue the access test", async () => {
  const backend = new MockBackend(); const save = vi.spyOn(backend, "saveApiConnection"); const check = vi.spyOn(backend, "testConnection"); const connected = vi.fn();
  const { container } = render(<ApiSetup backend={backend} proposal={proposal} connected={connected} />);
  expect(container.querySelector('input[type="password"]')).toBeNull();
  await userEvent.click(screen.getByRole("button", { name: "Save API connection" }));
  await screen.findByText(/Saved locally/); expect(save).toHaveBeenCalledWith(expect.objectContaining({ url: proposal.url, auth: "bearer" }));
  expect(check).not.toHaveBeenCalled();
  await userEvent.click(screen.getByRole("button", { name: "Test API access" }));
  await waitFor(() => expect(connected).toHaveBeenCalledWith(expect.objectContaining({ state: expect.objectContaining({ kind: "connected" }) })));
  expect(save.mock.calls[0][0]).not.toHaveProperty("credential");
});

test("Local only prevents research even when a model and review are available", async () => {
  const f = await fixture(); await f.backend.setLocalOnly(true); f.status = await f.backend.modelAccess();
  const research = vi.spyOn(f.backend, "researchTaskAccess");
  render(<ApiAccessPanel {...f} accept={vi.fn()} onBusy={vi.fn()} />);
  expect(screen.getByRole("button", { name: "Send and check access" })).toBeDisabled(); expect(research).not.toHaveBeenCalled();
});

test("research instructions and source metadata render as inert text", async () => {
  const payload = '<img src="https://evil.invalid/x" onerror="alert(1)">';
  const { container } = await panel({ ...report("needs_connection", { ...proposal, setupInstructions: [payload] }), summary: "<script>bad()</script>" });
  expect(screen.getByText(payload)).toBeVisible(); expect(screen.getByText("<script>bad()</script>")).toBeVisible();
  expect(container.querySelectorAll("img,script,iframe")).toHaveLength(0);
});

test("an old research response cannot replace a new draft", async () => {
  const f = await fixture(); let resolve!: (d: TaskDraft) => void;
  vi.spyOn(f.backend, "researchTaskAccess").mockImplementation(() => new Promise(r => { resolve = r; }));
  const accept = vi.fn(); const onBusy = vi.fn(); const view = render(<ApiAccessPanel {...f} accept={accept} onBusy={onBusy} />);
  await waitFor(() => expect(screen.getByRole("button", { name: "Send and check access" })).toBeEnabled());
  await userEvent.click(screen.getByRole("button", { name: "Send and check access" }));
  const replacement = await f.backend.draftTaskNoInput("A different task");
  view.rerender(<ApiAccessPanel backend={f.backend} draft={replacement} status={f.status} accept={accept} onBusy={onBusy} />);
  resolve(f.draft); await waitFor(() => expect(onBusy).toHaveBeenLastCalledWith(false)); expect(accept).not.toHaveBeenCalled();
});
