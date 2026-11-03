import {render,screen,waitFor} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import {expect,it,vi} from "vitest";
import {CreateFlow} from "./CreateFlow";
import {MockBackend} from "./backend/mock";
import type {BuildState, Program} from "./backend/types";

const NOTE=/gives the same result every run/;
const base:Program={summary:"s",output_name:"answer.txt",webSources:[],code:"function run(){}"};
async function ready(program:Program) {
  const backend=new MockBackend(),user=userEvent.setup();
  await backend.saveApiKey("test-key");
  vi.spyOn(backend,"startBuild").mockImplementation(async draftId=>({
    buildId:"b",draftId,route:"api_key",routeLabel:"Meta API key",phase:"ready",candidate:null,checks:[],events:[],spentMicros:0,failure:null,program,
    comparison:{inputText:"",outputText:"Out",inputColumns:[],inputRows:[],outputColumns:[],outputRows:[],rowsIn:0,rowsOut:0},
  } as BuildState));
  render(<CreateFlow backend={backend} initialInput={null} status={await backend.modelAccess()} connectors={[]} openSettings={vi.fn()} connect={vi.fn()} close={vi.fn()} saved={vi.fn(async()=>{})}/>);
  await user.type(screen.getByLabelText("What should the tool do?"),"Give me a workout");
  await user.click(screen.getByRole("button",{name:"Check the plan"}));
  await screen.findByRole("button",{name:"Check again"});
  await waitFor(()=>expect(screen.getByRole("button",{name:"Build and test"})).toBeEnabled());
  await user.click(screen.getByRole("button",{name:"Build and test"}));
  await screen.findByRole("button",{name:/change it/});
}
it("tells the person when a built tool can only give one result",async()=>{
  await ready(base);
  expect(screen.getByText(NOTE)).toBeInTheDocument();
});
it("says nothing when the tool asks the AI on each run",async()=>{
  await ready({...base,modelStep:{purpose:"Write it",prompt:"Write"}});
  expect(screen.queryByText(NOTE)).toBeNull();
});
it("says nothing when the tool reads a website",async()=>{
  await ready({...base,webSources:[{name:"w",url:"https://example.com/"}]});
  expect(screen.queryByText(NOTE)).toBeNull();
});
