import { describe, expect, it } from "vitest";
import { BackendError, toBackendError, normalizeWire } from "./tauri";

describe("toBackendError", () => {
  it("keeps the backend's safe message and code", () => {
    const e = toBackendError({ code: "invalid", message: "Gmail needs an OAuth client of your own." });
    expect(e).toBeInstanceOf(BackendError);
    expect(e.message).toBe("Gmail needs an OAuth client of your own.");
    expect((e as BackendError).code).toBe("invalid");
  });
  it("passes Errors through and wraps strings and unknowns", () => {
    const original = new Error("x");
    expect(toBackendError(original)).toBe(original);
    expect(toBackendError("plain").message).toBe("plain");
    expect(toBackendError(42).message).toBe("Something went wrong. Try again.");
  });
});

it("normalizes program fields without changing exact call arguments or remote schemas",()=>{
  const args={web_sources:"literal",program:{web_sources:["keep"],model_step:"literal"}};
  const schema={properties:{connection_sources:{type:"string"}}};
  const dto={program:{summary:"s",output_name:"summary.txt",code:"function run(){}",web_sources:[],connection_sources:[],model_step:{purpose:"Summarize"}},plannedCalls:[{call:{args}}],requiredConnections:[{tools:[{inputSchema:schema}]}]};
  expect(normalizeWire(dto)).toEqual({...dto,program:{summary:"s",output_name:"summary.txt",code:"function run(){}",webSources:[],connectionSources:[],modelStep:{purpose:"Summarize"}}});
  expect(normalizeWire({plannedCalls:[{call:{args}}]})).toEqual({plannedCalls:[{call:{args}}]});
});
