function run() { if (typeof Deno === "undefined") { macroscope.log("DENIED: Deno absent"); return; } Deno.readTextFileSync("deno.json"); macroscope.output("escaped.csv", "ESCAPED"); }
