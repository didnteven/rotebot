import { probe } from "./probe.ts";
await probe("env-home", () => Deno.env.get("HOME"));
