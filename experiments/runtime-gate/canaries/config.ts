// deno.json tries to grant this and defines an import-map-only alias.
import { probe } from "./probe.ts";
await probe("config", () => Deno.readTextFile("/etc/hosts"));
