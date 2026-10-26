import "file:///etc/hosts";
console.log(JSON.stringify({ escaped: true, canary: "static-hosts" }));
Deno.exit(42);
