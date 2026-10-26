import { probe } from "./probe.ts";
await probe("ffi", () => Deno.dlopen("/usr/lib/libSystem.B.dylib", {}));
