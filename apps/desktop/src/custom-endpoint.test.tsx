import {useState} from "react";
import {fireEvent,render,screen,waitFor} from "@testing-library/react";
import {expect,it,vi} from "vitest";
import {CustomEndpointCard} from "./CustomEndpointCard";
import {MockBackend} from "./backend/mock";
import {TauriBackend} from "./backend/tauri";
import type {ModelAccessStatus} from "./backend/types";
vi.mock("@tauri-apps/api/core",()=>({invoke:vi.fn()}));
vi.mock("@tauri-apps/api/event",()=>({listen:vi.fn()}));
const URL="https://toddhall34--test.modal.direct/v1";
const MODEL="meta-models/Muse-Spark-Open-816B-A42B";
async function show(backend=new MockBackend()) {
  const initial=await backend.modelAccess();
  function Card(){const [status,setStatus]=useState<ModelAccessStatus>(initial);return <CustomEndpointCard backend={backend} status={status.customEndpoint} update={setStatus}/>;}
  render(<Card/>);return backend;
}
function fill() {
  fireEvent.change(screen.getByRole("textbox",{name:"Endpoint base URL"}),{target:{value:URL}});
  fireEvent.change(screen.getByRole("textbox",{name:"Endpoint model"}),{target:{value:MODEL}});
}
async function save() {fireEvent.click(screen.getByRole("button",{name:"Save endpoint"}));await waitFor(()=>expect(screen.getByRole("button",{name:"Set token…"})).toBeEnabled());}
it("validates HTTPS URL, userinfo, query, fragment and required model before saving",async()=>{
  await show();const button=screen.getByRole("button",{name:"Save endpoint"});expect(button).toBeDisabled();fill();expect(button).toBeEnabled();
  for(const value of ["http://example.com/v1","https://user:secret@example.com/v1","https://@example.com/v1","https://example.com/v1?token=secret","https://example.com/v1#fragment","invalid"]) {
    fireEvent.change(screen.getByRole("textbox",{name:"Endpoint base URL"}),{target:{value}});expect(button).toBeDisabled();
  }
  fill();fireEvent.change(screen.getByRole("textbox",{name:"Endpoint model"}),{target:{value:""}});expect(button).toBeDisabled();
});
it("calls native token entry without rendering a token field",async()=>{
  const backend=await show();const native=vi.spyOn(backend,"saveCustomEndpointToken");
  fireEvent.click(screen.getByRole("button",{name:"Set token…"}));
  await waitFor(()=>expect(native).toHaveBeenCalledWith());expect(document.querySelector('input[type="password"]')).toBeNull();
  expect(await screen.findByText(/Token: Set/)).toBeVisible();
});
it("shows test result and latency and enables selecting the tested route",async()=>{
  const backend=await show();const select=vi.spyOn(backend,"selectRoute");fill();await save();
  expect(screen.getByRole("button",{name:"Use this route"})).toBeDisabled();
  fireEvent.click(screen.getByRole("button",{name:"Set token…"}));await waitFor(()=>expect(screen.getByRole("button",{name:"Test endpoint"})).toBeEnabled());
  fireEvent.click(screen.getByRole("button",{name:"Test endpoint"}));expect(await screen.findByText("Last endpoint test: OK · 7 ms")).toBeVisible();
  fireEvent.click(screen.getByRole("button",{name:"Use this route"}));await waitFor(()=>expect(select).toHaveBeenCalledWith("custom_endpoint"));
  expect((await backend.modelAccess()).effectiveRoute).toBe("custom_endpoint");expect(screen.getByText(/toddhall34--test.modal.direct/)).toBeVisible();
});
it("edits the call cap and display name and removes configuration",async()=>{
  const backend=await show();const options=vi.spyOn(backend,"setCustomEndpointOptions");fill();
  fireEvent.change(screen.getByRole("spinbutton",{name:"Monthly endpoint call cap"}),{target:{value:"72"}});
  fireEvent.change(screen.getByRole("textbox",{name:"Endpoint display name"}),{target:{value:"Owner Modal"}});await save();
  await waitFor(()=>expect(options).toHaveBeenCalledWith("Owner Modal",72));expect((await backend.modelAccess()).customEndpoint.monthlyCallCap).toBe(72);
  fireEvent.change(screen.getByRole("spinbutton",{name:"Monthly endpoint call cap"}),{target:{value:"-1"}});expect(screen.getByRole("button",{name:"Save endpoint"})).toBeDisabled();
  fireEvent.click(screen.getByRole("button",{name:"Remove"}));await waitFor(()=>expect(screen.getByRole("textbox",{name:"Endpoint base URL"})).toHaveValue(""));
});
it("displays auth rejection from the backend and keeps route selection disabled",async()=>{
  const backend=new MockBackend();await backend.saveCustomEndpoint(URL,MODEL);await backend.saveCustomEndpointToken();
  vi.spyOn(backend,"testCustomEndpoint").mockImplementation(async()=>({...await backend.modelAccess(),customEndpoint:{...(await backend.modelAccess()).customEndpoint,lastTest:{status:"token rejected",latencyMs:32}}}));
  await show(backend);fireEvent.click(screen.getByRole("button",{name:"Test endpoint"}));
  expect(await screen.findByText("Last endpoint test: token rejected · 32 ms")).toBeVisible();expect(screen.getByRole("button",{name:"Use this route"})).toBeDisabled();
});
it("native IPC sends no credential argument",async()=>{
  const {invoke}=await import("@tauri-apps/api/core");const backend=new TauriBackend();await backend.saveCustomEndpointToken();expect(invoke).toHaveBeenCalledWith("save_custom_endpoint_token");
});
