function run() { if (typeof Deno === "undefined") { macroscope.log("DENIED: Deno absent"); return; } Deno.readTextFileSync("/etc/hosts"); macroscope.output("escaped.csv", "ESCAPED"); }
