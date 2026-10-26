import "config-must-be-ignored";
console.log(JSON.stringify({ escaped: true, canary: "config-import-map" }));
Deno.exit(42);
