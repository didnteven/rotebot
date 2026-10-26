import { probe } from "./probe.ts";
await probe("write", () => Deno.writeTextFile("__OUTSIDE_WRITE__", "synthetic escape\n"));
