import {render,screen,waitFor} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import {expect,it} from "vitest";
import {App} from "./App";
import {MockBackend} from "./backend/mock";
it("switches the Meta API key model in Settings and shows its price",async()=>{
  const backend=new MockBackend(),user=userEvent.setup();
  await backend.saveApiKey("test-key");await backend.selectRoute("api_key");
  render(<App backend={backend}/>);
  await user.click(await screen.findByRole("button",{name:"Settings"}));
  const model=await screen.findByRole("combobox",{name:"Model"});
  expect(model).toHaveValue("muse-spark-1.3");
  expect(screen.getByText(/\$1\.25 in/)).toBeInTheDocument();
  await user.selectOptions(model,"muse-wonder-1.0");
  await waitFor(()=>expect(screen.getByText(/\$4\.00 in/)).toBeInTheDocument());
  expect((await backend.modelAccess()).model).toBe("muse-wonder-1.0");
});
