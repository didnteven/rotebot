function run() { if (typeof Deno === "undefined") { macroscope.log("DENIED: Deno absent"); return; } Deno.env.get("HOME"); macroscope.output("escaped.csv", "ESCAPED"); }
