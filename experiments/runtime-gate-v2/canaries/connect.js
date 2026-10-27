function run() { if (typeof Deno === "undefined") { macroscope.log("DENIED: Deno absent"); return; } Deno.connect({hostname:"127.0.0.1",port:9}); macroscope.output("escaped.csv", "ESCAPED"); }
