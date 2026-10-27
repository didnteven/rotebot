function run() { if (typeof fetch === "undefined") { macroscope.log("DENIED: fetch absent"); return; } fetch("https://example.com"); macroscope.output("escaped.csv", "ESCAPED"); }
