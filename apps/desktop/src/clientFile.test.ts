import { expect, it } from "vitest";
import { googleConnectorIds, parseGoogleClientFile } from "./clientFile";

it("reads the client ID and secret from Google's downloaded file, desktop or web", () => {
  const installed = JSON.stringify({ installed: { client_id: "1-abc.apps.googleusercontent.com", client_secret: "s3", redirect_uris: ["http://localhost"] } });
  expect(parseGoogleClientFile(installed)).toEqual({ clientId: "1-abc.apps.googleusercontent.com", clientSecret: "s3" });
  const web = JSON.stringify({ web: { client_id: "2-def.apps.googleusercontent.com" } });
  expect(parseGoogleClientFile(web)).toEqual({ clientId: "2-def.apps.googleusercontent.com", clientSecret: null });
});

it("rejects files that are not a Google OAuth client", () => {
  expect(parseGoogleClientFile("not json")).toBeNull();
  expect(parseGoogleClientFile(JSON.stringify({ installed: { client_id: "plain-id" } }))).toBeNull();
  expect(parseGoogleClientFile(JSON.stringify({ type: "service_account", client_id: "x.apps.googleusercontent.com" }))).toBeNull();
  expect(googleConnectorIds).toContain("gmail");
});
