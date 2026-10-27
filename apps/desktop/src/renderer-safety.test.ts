import { readFileSync, readdirSync } from "node:fs";
import { join, resolve } from "node:path";
import { expect, it } from "vitest";

it("greps every source file for unsafe HTML rendering APIs", () => {
  const root = resolve(process.cwd(), "src");
  const forbidden = ["dangerously" + "SetInnerHTML", "inner" + "HTML"];
  function scan(dir: string): string[] {
    return readdirSync(dir, { withFileTypes: true }).flatMap((entry) =>
      entry.isDirectory()
        ? scan(join(dir, entry.name))
        : [join(dir, entry.name)],
    );
  }
  const violations = scan(root).flatMap((file) =>
    forbidden
      .filter((word) => readFileSync(file, "utf8").includes(word))
      .map((word) => `${file}: ${word}`),
  );
  expect(violations).toEqual([]);
});

it("pins the product CSP to local resources and IPC with document escape routes denied", () => {
  const config = JSON.parse(readFileSync(
    resolve(process.cwd(), "src-tauri/tauri.conf.json"), "utf8",
  ));
  const directives = config.app.security.csp.split(";").map((part: string) => part.trim().split(/\s+/));
  expect(directives).toEqual([
    ["default-src", "'self'"],
    ["script-src", "'self'"],
    ["style-src", "'self'", "'unsafe-inline'"],
    ["img-src", "'self'", "data:"],
    ["connect-src", "ipc:", "http://ipc.localhost"],
    ["object-src", "'none'"],
    ["base-uri", "'none'"],
    ["form-action", "'none'"],
    ["frame-ancestors", "'none'"],
  ]);
  expect(config.app.security.capabilities).toEqual(["main"]);
});
