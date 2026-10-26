import data from "./private-link.json" with { type: "json" };
console.log(JSON.stringify({ escaped: true, canary: "symlink-private", marker: data.marker }));
Deno.exit(42);
