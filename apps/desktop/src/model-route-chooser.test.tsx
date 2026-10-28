import {render, screen, waitFor, within} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import {expect, it, vi} from "vitest";
import {App} from "./App";
import {MockBackend} from "./backend/mock";

it("puts all model routes first with credential states and inline repair actions", async()=>{
  const backend = new MockBackend(), user = userEvent.setup();
  const refresh = vi.spyOn(backend,"refreshMuseStatus");
  render(<App backend={backend}/>);
  await user.click(screen.getByRole("button",{name:"Settings"}));
  const chooser = await screen.findByRole("group",{name:"Which model builds your tools"});
  expect(chooser).toHaveFocus();
  expect(chooser.closest("section")?.querySelector("fieldset")).toBe(chooser);
  expect(Array.from(chooser.closest("section")!.querySelectorAll("h2"))[0]).toHaveTextContent("Model access");
  const radios = within(chooser).getAllByRole("radio");
  expect(radios.map(r=>r.getAttribute("value"))).toEqual(["muse_plan","api_key","custom_endpoint","offline_demo"]);
  for (const name of [/Muse plan/,/Meta API key/,/Custom endpoint/]) expect(within(chooser).getByRole("radio",{name})).toBeDisabled();
  expect(within(chooser).getByText("Not signed in")).toBeVisible();
  expect(within(chooser).getByText("Needs a key")).toBeVisible();
  expect(within(chooser).getByText("Needs a token and a passing test")).toBeVisible();
  expect(within(chooser).getByText("Ready")).toBeVisible();
  await user.click(within(chooser).getByRole("button",{name:"Check sign-in"}));
  expect(refresh).toHaveBeenCalledOnce();
  await user.click(within(chooser).getByRole("button",{name:"Set up…"}));
  expect(screen.getByRole("textbox",{name:"Endpoint base URL"})).toHaveFocus();
  await user.click(within(chooser).getByRole("button",{name:"Add key…"}));
  expect(screen.getByLabelText("Meta API key")).toHaveFocus();
});

it("Change in Create reaches the chooser, switches immediately, and preserves the draft",async()=>{
  const backend = new MockBackend(), user = userEvent.setup();
  await backend.selectRoute("offline_demo");
  await backend.saveApiKey("test-key");
  const select = vi.spyOn(backend,"selectRoute");
  render(<App backend={backend}/>);
  await user.click(await screen.findByRole("button",{name:"Describe a task"}));
  await user.type(screen.getByLabelText("What should the tool do?"),"Keep this task description");
  await user.click(screen.getByRole("button",{name:"Settings for model access"}));
  const chooser = await screen.findByRole("group",{name:"Which model builds your tools"});
  expect(chooser).toHaveFocus();
  await user.click(within(chooser).getByRole("radio",{name:/Meta API key/}));
  await screen.findByText("Builds now use Meta API key");
  expect(select).toHaveBeenCalledExactlyOnceWith("api_key");
  expect(within(chooser).getByRole("radio",{name:/Meta API key/})).toBeChecked();
  await user.click(screen.getByRole("button",{name:"My tools"}));
  expect(screen.getByLabelText("What should the tool do?")).toHaveValue("Keep this task description");
  await waitFor(()=>expect(screen.getByRole("region",{name:"App status"})).toHaveTextContent("Using Meta API key"));
});

it("a tested custom endpoint can be selected from the chooser",async()=>{
  const backend=new MockBackend(),user=userEvent.setup();
  await backend.saveCustomEndpoint("https://models.example.invalid/v1","test-model");
  await backend.saveCustomEndpointToken();
  await backend.testCustomEndpoint();
  render(<App backend={backend}/>);
  await user.click(screen.getByRole("button",{name:"Settings"}));
  const chooser=await screen.findByRole("group",{name:"Which model builds your tools"});
  const custom=within(chooser).getByRole("radio",{name:"Custom endpoint"});
  expect(custom).toBeEnabled();
  expect(custom.closest("label")?.parentElement).toHaveTextContent("Ready");
  await user.click(custom);
  expect(await screen.findByText("Builds now use Custom endpoint")).toBeVisible();
  expect((await backend.modelAccess()).effectiveRoute).toBe("custom_endpoint");
});

it("a ready Muse row switches and confirms the selection",async()=>{
  const backend=new MockBackend(), user=userEvent.setup();
  const now=vi.spyOn(Date,"now").mockReturnValue(1000);
  await backend.openMuseSignIn(); // In-memory demo timer only.
  now.mockReturnValue(1800);
  await backend.refreshMuseStatus();
  now.mockRestore();
  await backend.saveApiKey("test-key");
  const select=vi.spyOn(backend,"selectRoute");
  render(<App backend={backend}/>);
  await user.click(screen.getByRole("button",{name:"Settings"}));
  const chooser=await screen.findByRole("group",{name:"Which model builds your tools"});
  const muse=within(chooser).getByRole("radio",{name:/Muse plan/});
  expect(muse).toBeEnabled();
  expect(muse.closest("label")?.parentElement).toHaveTextContent("Ready");
  await user.click(muse);
  expect(select).toHaveBeenCalledExactlyOnceWith("muse_plan");
  expect(await screen.findByText("Builds now use Muse plan")).toBeVisible();
  expect(muse).toBeChecked();
  expect((await backend.modelAccess()).effectiveRoute).toBe("muse_plan");
  await user.click(within(chooser).getByRole("radio",{name:/Meta API key/}));
  expect(await screen.findByText("Builds now use Meta API key")).toBeVisible();
  expect((await backend.modelAccess()).effectiveRoute).toBe("api_key");
});
