import { botIdentity } from "./Bot";
import {render,screen,waitFor,within} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import {expect,it} from "vitest";
import {App} from "./App";
import {MockBackend} from "./backend/mock";

const button=(name:string|RegExp)=>screen.getByRole("button",{name});
const GOAL="Get today's forecast for Auckland from https://api.open-meteo.com and write a short summary to weather.txt";

async function createWeather(user:ReturnType<typeof userEvent.setup>) {
  await user.click(button("Describe a task"));
  const description=screen.getByLabelText("What should the tool do?");await user.clear(description);await user.type(description,GOAL);
  await user.click(button("Check the plan"));
  await waitFor(()=>expect(button("Build and test")).toBeEnabled());
  await user.click(button("Build and test"));
  await waitFor(()=>expect(button("Yes, save this tool")).toBeVisible(),{timeout:5000});
  const name=screen.getByLabelText("Tool name");await user.clear(name);await user.type(name,"Weather");
  await user.click(button("Yes, save this tool"));
  await screen.findByRole("heading",{name:/Weather/});
}
async function open() {
  const backend=new MockBackend();await backend.saveApiKey("test-key");
  const user=userEvent.setup();render(<App backend={backend}/>);
  await waitFor(()=>expect(button("Describe a task")).toBeEnabled());
  return {backend,user};
}

it("edits a saved task: description comes back, saving adds a version to the same tool, and the old one can be restored",async()=>{
  const {backend,user}=await open();await createWeather(user);
  expect((await backend.listTools()).map(t=>t.version)).toEqual([1]);

  await user.click(button("Edit: Weather"));
  expect(await screen.findByText(/Editing “Weather”/)).toBeVisible();
  const description=screen.getByLabelText("What should the tool do?");expect(description).toHaveValue(GOAL);
  await user.type(description," Also show the low.");
  await user.click(button("Check the plan"));
  await waitFor(()=>expect(button("Build and test")).toBeEnabled());
  await user.click(button("Build and test"));
  await waitFor(()=>expect(button("Yes, save changes")).toBeVisible(),{timeout:5000});
  expect(screen.getByLabelText("Tool name")).toHaveValue("Weather");
  expect(screen.queryByRole("button",{name:"Yes, save this tool"})).toBeNull();
  await user.click(button("Yes, save changes"));

  await screen.findByRole("heading",{name:/Weather/});
  const tools=await backend.listTools();expect(tools).toHaveLength(1);expect(tools[0].version).toBe(2);expect(tools[0].name).toBe("Weather");
  expect((await backend.getTool(tools[0].id)).goal).toContain("Also show the low.");
  expect(screen.getAllByRole("heading",{name:/Weather/})).toHaveLength(1);

  await user.click(screen.getByLabelText("More actions: Weather"));
  await user.click(button("Version history"));
  const list=await screen.findByRole("list",{name:"Versions of Weather"});
  expect(within(list).getAllByRole("listitem")).toHaveLength(2);
  expect(within(list).getByText(/Version 2 \(current\)/)).toBeVisible();
  expect(within(list).queryByRole("button",{name:/Restore version 2/})).toBeNull();
  await user.click(button("Restore version 1 of Weather"));
  await waitFor(async()=>expect((await backend.listTools())[0].version).toBe(1));
  expect((await backend.getTool(tools[0].id)).goal).toBe(GOAL);
  expect(await within(await screen.findByRole("list",{name:"Versions of Weather"})).findByText(/Version 1 \(current\)/)).toBeVisible();
});

it("leaving an edit without saving changes nothing",async()=>{
  const {backend,user}=await open();await createWeather(user);
  await user.click(button("Edit: Weather"));
  await user.type(screen.getByLabelText("What should the tool do?")," Changed my mind.");
  await user.click(button("Back"));
  await screen.findByRole("heading",{name:/Weather/});
  const [tool]=await backend.listTools();expect(tool.version).toBe(1);
  expect(await backend.listToolVersions(tool.id)).toHaveLength(1);
  // A later new task is a new task, not an edit.
  await user.click(button("Describe a task"));
  expect(screen.queryByText(/Editing “Weather”/)).toBeNull();expect(screen.getByLabelText("What should the tool do?")).not.toHaveValue(GOAL);
});

it("the sample tool cannot be edited",async()=>{
  const backend=new MockBackend();await backend.installSampleTool();
  render(<App backend={backend}/>);
  await screen.findByRole("heading",{name:/Clean import CSV/});
  expect(screen.queryByRole("button",{name:/^Edit:/})).toBeNull();
  const [sample]=await backend.listTools();
  const draft=await backend.draftTaskNoInput("anything");
  await expect(backend.setDraftReplaces(draft.draftId,sample.id)).rejects.toThrow(/sample/i);
});

it("saves a changed bot appearance on Back without rebuilding or changing the task version", async () => {
  const {backend,user}=await open();await createWeather(user);
  const [tool]=await backend.listTools();
  const previous=botIdentity(tool.id);
  await user.click(button("Edit: Weather"));
  await user.click(await screen.findByRole("button",{name:"Edit bot appearance"}));
  await user.click(screen.getByRole("menuitem",{name:"Try another bot"}));
  const chosen=botIdentity(tool.id);
  expect(chosen).not.toBe(previous);
  expect(screen.getByRole("status")).toHaveTextContent("Bot appearance saved");
  await user.click(button("Back"));
  await screen.findByRole("heading",{name:/Weather/});
  expect(botIdentity(tool.id)).toBe(chosen);
  expect((await backend.getTool(tool.id)).version).toBe(1);
  await user.click(button("Edit: Weather"));
  expect(botIdentity(tool.id)).toBe(chosen);
});
