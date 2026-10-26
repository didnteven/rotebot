import "https://example.com/runtime-gate-canary.ts";
console.log(JSON.stringify({ escaped: true, canary: "static-remote" }));
Deno.exit(42);
