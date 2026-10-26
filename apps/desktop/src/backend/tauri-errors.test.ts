import { describe, expect, it } from "vitest";
import { BackendError, toBackendError } from "./tauri";

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
