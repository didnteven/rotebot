import { probe } from "./probe.ts";
await probe("fetch", () => fetch("https://example.com"));
