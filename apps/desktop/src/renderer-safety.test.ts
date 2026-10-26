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
