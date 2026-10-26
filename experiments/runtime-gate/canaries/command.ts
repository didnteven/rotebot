import { probe } from "./probe.ts";
await probe("command", () => new Deno.Command("/bin/sh", { args: ["-c", "exit 0"] }).output());
