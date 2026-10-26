import { probe } from "./probe.ts";
await probe("dynamic-remote", () => import("https://example.com/runtime-gate-canary.ts"));
