import { probe } from "./probe.ts";
await probe("connect", async () => { (await Deno.connect({ hostname: "127.0.0.1", port: 9 })).close(); });
