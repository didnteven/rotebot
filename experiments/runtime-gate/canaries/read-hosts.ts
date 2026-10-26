import { probe } from "./probe.ts";
await probe("read-hosts", () => Deno.readTextFile("/etc/hosts"));
