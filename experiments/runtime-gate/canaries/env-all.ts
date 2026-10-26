import { probe } from "./probe.ts";
await probe("env-all", () => Deno.env.toObject());
