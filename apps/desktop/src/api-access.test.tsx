import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, expect, test, vi } from "vitest";
import { ApiAccessPanel, ApiSetup } from "./ApiAccess";
import { Connections } from "./Connections";
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
  await waitFor(() => expect(screen.getByRole("button", { name: "Check again" })).toBeEnabled());
  return { ...f, accept, onBusy, ...view };
}

test("with an AI service set up, the data check runs on its own once, without a consent button", async () => {
  const f = await fixture(); const research = vi.spyOn(f.backend, "researchTaskAccess");
  render(<ApiAccessPanel {...f} accept={vi.fn()} onBusy={vi.fn()} />);
  expect(screen.queryByRole("button", { name: "Send and check access" })).not.toBeInTheDocument();
  await waitFor(() => expect(research).toHaveBeenCalledWith(f.draft.draftId, expect.any(String)));
  expect(research).toHaveBeenCalledTimes(1);
});

test("a known public API needs no connection or token setup box", async () => {
  await panel(report("public_api", { ...proposal, auth: "none" }));
  // One line says where the data comes from; the reasoning, address and steps are folded away.
  expect(screen.getByText("Data from Example records")).toBeVisible();
  expect(screen.getByText("Access check result")).not.toBeVisible();
  expect(screen.queryByText("Setup steps")).not.toBeInTheDocument();
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

test("when existing keys already work at the proposed address, one click adds it and selects it", async () => {
  const f = await fixture(); const source = await f.backend.saveApiConnection({ name: "Ahaku", url: "https://api.example.com/me", auth: "bearer", headerName: "", docsUrl: null }, { main: "k", extras: [] });
  f.draft.accessPlan = { ...report("same_service_works", proposal), reuseKeysFrom: source.id, summary: "Your keys already work on this service." };
  const add = vi.spyOn(f.backend, "addApiAddress"); const select = vi.spyOn(f.backend, "selectTaskConnections");
  render(<ApiAccessPanel {...f} accept={vi.fn()} onBusy={vi.fn()} />);
  expect(screen.getByText("Your existing keys work here")).toBeVisible();
  expect(screen.queryByRole("button", { name: "Save API connection" })).not.toBeInTheDocument();
  await userEvent.click(screen.getByRole("button", { name: "Use my existing connection for this" }));
  await waitFor(() => expect(add).toHaveBeenCalledWith(source.id, proposal.url));
  await waitFor(() => expect(select).toHaveBeenCalled());
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

test("keys are typed into hidden boxes, cleared after saving, and saving does not issue the access test", async () => {
  const backend = new MockBackend(); const save = vi.spyOn(backend, "saveApiConnection"); const check = vi.spyOn(backend, "testConnection"); const connected = vi.fn();
  const { container } = render(<ApiSetup backend={backend} proposal={proposal} connected={connected} />);
  expect(screen.getByRole("button", { name: "Save API connection" })).toBeDisabled();
  await userEvent.type(container.querySelector('input[type="password"]')!, "SYNTHETIC-TOKEN");
  await userEvent.click(screen.getByRole("button", { name: "Save API connection" }));
  await screen.findByText(/Saved on this Mac/); expect(save).toHaveBeenCalledWith(expect.objectContaining({ url: proposal.url, auth: "bearer" }), { main: "SYNTHETIC-TOKEN", extras: [] });
  expect(container.textContent).not.toContain("SYNTHETIC-TOKEN"); expect(container.querySelector('input[type="password"]')).toBeNull();
  expect(check).not.toHaveBeenCalled();
  await userEvent.click(screen.getByRole("button", { name: "Test API access" }));
  await waitFor(() => expect(connected).toHaveBeenCalledWith(expect.objectContaining({ state: expect.objectContaining({ kind: "connected" }) })));
  expect(save.mock.calls[0][0]).not.toHaveProperty("credential");
});

test("Local only prevents research even when a model and review are available", async () => {
  const f = await fixture(); await f.backend.setLocalOnly(true); f.status = await f.backend.modelAccess();
  const research = vi.spyOn(f.backend, "researchTaskAccess");
  render(<ApiAccessPanel {...f} accept={vi.fn()} onBusy={vi.fn()} />);
  await screen.findByText(/Set up an AI service/); expect(research).not.toHaveBeenCalled();
});

test("research instructions and source metadata render as inert text", async () => {
  const payload = '<img src="https://evil.invalid/x" onerror="alert(1)">';
  const { container } = await panel({ ...report("needs_connection", { ...proposal, setupInstructions: [payload] }), summary: "<script>bad()</script>" });
  expect(screen.getByText(payload)).toBeVisible(); expect(screen.getByText("<script>bad()</script>")).toBeVisible();
  expect(container.querySelectorAll("img,script,iframe")).toHaveLength(0);
});

test("an old research response cannot replace a new draft", async () => {
  const f = await fixture(); const resolvers: ((d: TaskDraft) => void)[] = [];
  vi.spyOn(f.backend, "researchTaskAccess").mockImplementation(() => new Promise(r => { resolvers.push(r); }));
  const accept = vi.fn(); const onBusy = vi.fn(); const view = render(<ApiAccessPanel {...f} accept={accept} onBusy={onBusy} />);
  await waitFor(() => expect(f.backend.researchTaskAccess).toHaveBeenCalled());
  const replacement = await f.backend.draftTaskNoInput("A different task");
  view.rerender(<ApiAccessPanel backend={f.backend} draft={replacement} status={f.status} accept={accept} onBusy={onBusy} />);
  resolvers[0](f.draft); await new Promise(r => setTimeout(r, 50)); expect(accept).not.toHaveBeenCalledWith(f.draft);
});

test("an API that needs an app id beside the token asks for both keys and sends only header names", async () => {
  const backend = new MockBackend(); const save = vi.spyOn(backend, "saveApiConnection");
  const { container } = render(<ApiSetup backend={backend} proposal={{ ...proposal, extraHeaderNames: ["X-App-Id"] }} />);
  const boxes = container.querySelectorAll('input[type="password"]');
  expect(boxes).toHaveLength(2);
  expect(screen.getByText(/Second key: the value for X-App-Id/)).toBeVisible();
  await userEvent.type(boxes[0] as HTMLElement, "T1"); await userEvent.type(boxes[1] as HTMLElement, "A1");
  await userEvent.click(screen.getByRole("button", { name: "Save API connection" }));
  await screen.findByText(/Saved on this Mac/);
  expect(save).toHaveBeenCalledWith(expect.objectContaining({ extraHeaderNames: ["X-App-Id"] }), { main: "T1", extras: ["A1"] });
});

test("a proposal's technical fields are tucked away, while a manual custom API shows them", async () => {
  const backend = new MockBackend();
  const first = render(<ApiSetup backend={backend} proposal={proposal} />);
  expect(first.container.querySelector("details")).not.toHaveAttribute("open"); first.unmount();
  const second = render(<ApiSetup backend={backend} />);
  expect(second.container.querySelector("details")).toHaveAttribute("open");
});

test("custom APIs have their own visible section with the saved ones and an add button", async () => {
  const backend = new MockBackend();
  await backend.saveApiConnection({ name: "Bank accounts", url: "https://api.example.com/accounts", auth: "none", headerName: "", docsUrl: null });
  render(<Connections backend={backend} connectors={await backend.listConnectors()} selected={null} select={vi.fn()} changed={vi.fn(async () => {})} />);
  expect(screen.getByRole("heading", { name: "Custom APIs" })).toBeVisible();
  expect(screen.getByRole("heading", { name: "Bank accounts" })).toBeVisible();
  expect(screen.queryByRole("region", { name: "Custom API setup" })).not.toBeInTheDocument();
  await userEvent.click(screen.getByRole("button", { name: "Add a custom API" }));
  expect(screen.getByRole("region", { name: "Custom API setup" })).toBeVisible();
});

test("each proposal says in plain words what it costs, and never claims a cost the documentation did not give", async () => {
  const free = await panel(report("needs_connection", { ...proposal, cost: "free_personal" }));
  expect(screen.getByText("Free for your own accounts or data")).toBeVisible(); free.unmount();
  const paid = await panel(report("needs_connection", { ...proposal, cost: "paid_or_approval" }));
  expect(screen.getByText("May cost money or need the provider's approval")).toBeVisible(); paid.unmount();
  await panel(report("needs_connection", proposal));
  expect(screen.getByText("Cost not confirmed in the documentation")).toBeVisible();
});

test("two keys are set up from the generic fields, naming each box in turn", async () => {
  const backend = new MockBackend(); const save = vi.spyOn(backend, "saveApiConnection");
  const { container } = render(<ApiSetup backend={backend} />);
  await userEvent.type(screen.getByLabelText("Name"), "Bank accounts");
  await userEvent.type(screen.getByLabelText(/^API URL/), "https://api.example.com/v1/accounts");
  await userEvent.type(screen.getByLabelText(/Second key/), "X-App-Id");
  const boxes = container.querySelectorAll('input[type="password"]');
  await userEvent.type(boxes[0] as HTMLElement, "T1"); await userEvent.type(boxes[1] as HTMLElement, "A1");
  await userEvent.click(screen.getByRole("button", { name: "Save API connection" }));
  await screen.findByText(/Saved on this Mac/);
  expect(save).toHaveBeenCalledWith(expect.objectContaining({ auth: "bearer", extraHeaderNames: ["X-App-Id"] }), { main: "T1", extras: ["A1"] });
});

test("a saved API can be changed or followed by another one instead of freezing the form", async () => {
  const backend = new MockBackend(); const save = vi.spyOn(backend, "saveApiConnection"); const remove = vi.spyOn(backend, "disconnect");
  const { container } = render(<ApiSetup backend={backend} />);
  const fill = async (name: string, key: string) => {
    await userEvent.clear(screen.getByLabelText("Name")); await userEvent.type(screen.getByLabelText("Name"), name);
    await userEvent.clear(screen.getByLabelText(/^API URL/)); await userEvent.type(screen.getByLabelText(/^API URL/), "https://api.example.com/v1/items");
    await userEvent.type(container.querySelector('input[type="password"]')!, key);
    await userEvent.click(screen.getByRole("button", { name: "Save API connection" })); await screen.findByText(/Saved on this Mac/);
  };
  await fill("First", "K1");
  await userEvent.click(screen.getByRole("button", { name: "Change details" }));
  expect(remove).toHaveBeenCalledTimes(1);
  expect(screen.getByLabelText("Name")).toHaveValue("First"); expect(screen.getByLabelText("Name")).toBeEnabled();
  await fill("First fixed", "K2");
  await userEvent.click(screen.getByRole("button", { name: "Add another API" }));
  expect(screen.getByLabelText("Name")).toHaveValue(""); await fill("Second", "K3");
  expect(save).toHaveBeenCalledTimes(3);
});

test("editing a saved API fills in its settings, keeps the saved keys unless new ones are typed, and replaces the old connection", async () => {
  const backend = new MockBackend(); const save = vi.spyOn(backend, "saveApiConnection");
  const old = await backend.saveApiConnection({ name: "Bank", url: "https://api.example.com/v1", auth: "bearer", headerName: "", docsUrl: null, extraHeaderNames: ["X-App-Id"] }, { main: "a", extras: ["b"] });
  const config = await backend.getApiConfig(old.id);
  const { container } = render(<ApiSetup backend={backend} edit={{ id: old.id, config }} />);
  expect(screen.getByLabelText("Name")).toHaveValue("Bank");
  expect(screen.getByLabelText(/^API URL/)).toHaveValue("https://api.example.com/v1");
  expect(container.querySelectorAll('input[type="password"]')).toHaveLength(2);
  expect([...container.querySelectorAll('input[type="password"]')].map(b => (b as HTMLInputElement).value)).toEqual(["••••••••••", "••••••••••"]);
  await userEvent.clear(screen.getByLabelText(/^API URL/)); await userEvent.type(screen.getByLabelText(/^API URL/), "https://api.example.com/v1/accounts");
  await userEvent.click(screen.getByRole("button", { name: "Save API connection" }));
  await waitFor(() => expect(save).toHaveBeenCalledTimes(2));
  // The old connection is replaced, not left beside the new one.
  expect((await backend.listConnectors()).filter(c => c.registration === "api").map(c => c.id)).not.toContain(old.id);
  // Blank boxes tell the host to keep the saved keys of the connection being replaced.
  expect(save).toHaveBeenLastCalledWith(expect.objectContaining({ url: "https://api.example.com/v1/accounts", extraHeaderNames: ["X-App-Id"] }), { main: "", extras: [""] }, old.id);
});

test("one button explains the problem, finds and tests addresses, and offers a one-click update", async () => {
  const backend = new MockBackend();
  const info = await backend.saveApiConnection({ name: "Bank accounts", url: "https://api.example.com/v1", auth: "none", headerName: "", docsUrl: null });
  const changed = vi.fn(async () => {});
  render(<Connections backend={backend} connectors={await backend.listConnectors()} selected={null} select={vi.fn()} changed={changed} />);
  await userEvent.click(screen.getByRole("button", { name: "Manage Bank accounts" }));
  await userEvent.click(screen.getByText(/Debug this connection/));
  const originalSuggest = backend.suggestApiExamples.bind(backend); let release!: () => void;
  vi.spyOn(backend, "suggestApiExamples").mockImplementation(async id => { await new Promise<void>(r => { release = r; }); return originalSuggest(id); });
  await userEvent.click(screen.getByRole("button", { name: "Check and fix this connection" }));
  const report = await screen.findByLabelText("Debug report");
  expect(report).toHaveTextContent("does not exist (404)"); expect(report).toHaveTextContent("The address itself is wrong");
  expect(await screen.findByText(/Looking for addresses that work/)).toBeVisible();
  release();
  const found = await screen.findByLabelText("Working address found");
  expect(found).toHaveTextContent("https://api.example.com/accounts");
  const add = vi.spyOn(backend, "addApiAddress");
  await userEvent.click(within(found).getByRole("button", { name: "Use this path on this connection" }));
  await waitFor(() => expect(add).toHaveBeenCalledWith(info.id, "https://api.example.com/accounts"));
  expect(changed).toHaveBeenCalled();
  // Still one connection: the path is remembered on it, not added as a second one.
  expect((await backend.listConnectors()).filter(c => c.registration === "api")).toHaveLength(1);
});

test("when every example fails the person is told to re-check their keys and shown what each call said", async () => {
  const backend = new MockBackend();
  const info = await backend.saveApiConnection({ name: "Bank accounts", url: "https://api.example.com/v1/items", auth: "none", headerName: "", docsUrl: null });
  await backend.suggestApiExamples(info.id);
  vi.spyOn(backend, "listApiExamples").mockResolvedValue([
    { label: "List accounts", url: "https://api.example.com/accounts", note: "", ok: false, result: "API authentication failed; check the saved credential. Nothing ran.", testedAtMs: 1 },
    { label: "Profile", url: "https://api.example.com/me", note: "", ok: false, result: "The service says that address does not exist (404).", testedAtMs: 1 },
  ]);
  render(<Connections backend={backend} connectors={await backend.listConnectors()} selected={null} select={vi.fn()} changed={vi.fn(async () => {})} />);
  await userEvent.click(screen.getByRole("button", { name: "Manage Bank accounts" }));
  await userEvent.click(screen.getByText(/Debug this connection/));
  const alert = await screen.findByRole("alert", { name: "Nothing worked" });
  expect(alert).toHaveTextContent("re-check your keys");
  expect(alert).toHaveTextContent("https://api.example.com/accounts: API authentication failed");
  expect(alert).toHaveTextContent("https://api.example.com/me: The service says that address does not exist");
  expect(screen.getByLabelText("List accounts")).toHaveTextContent("Failed: API authentication failed");
});

test("a failing API connection opens on its own with the debug check ready", async () => {
  const backend = new MockBackend();
  const info = await backend.saveApiConnection({ name: "Bank accounts", url: "https://api.example.com/v1", auth: "none", headerName: "", docsUrl: null });
  const failing = { ...info, state: { kind: "error" as const, message: "The service says that address does not exist (404)." } };
  render(<Connections backend={backend} connectors={[failing]} selected={null} select={vi.fn()} changed={vi.fn(async () => {})} />);
  expect(screen.getByRole("alert")).toHaveTextContent("does not exist (404)");
  expect(screen.getByRole("button", { name: "Check and fix this connection" })).toBeVisible();
});
