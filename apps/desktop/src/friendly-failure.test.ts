import { expect, it } from "vitest";
import { friendlyFailure } from "./ui";

it("explains a Developer Preview refusal instead of blaming a website", () => {
  const message = "Connection read was rejected by the service: Access to this tool requires that your Google Cloud project (1) is enrolled in the Google Workspace Developer Preview Program. Please see https://developers.google.com/workspace/preview";
  const text = friendlyFailure(message);
  expect(text).toContain("Developer Preview");
  expect(text).not.toContain("developers.google.com");
});

it("treats any other refused connection read as the service's refusal", () => {
  expect(friendlyFailure("Connection read was rejected by the service: Invalid argument at https://example.com/x")).toMatch(/connected service refused/);
});
