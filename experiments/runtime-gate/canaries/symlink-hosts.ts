import data from "./hosts-link.json" with { type: "json" };
console.log(JSON.stringify({ escaped: true, canary: "symlink-hosts", loaded: !!data }));
Deno.exit(42);
