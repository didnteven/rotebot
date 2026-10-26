import { probe } from "./probe.ts";
// The harness substitutes an absolute synthetic home path; no real SSH files.
await probe("read-home", () => Deno.readTextFile("__SYNTHETIC_HOME__/.ssh/known_hosts"));
