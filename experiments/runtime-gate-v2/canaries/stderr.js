function run() { if (typeof console === "undefined") { macroscope.log("DENIED: console absent"); return; } console.error("x".repeat(1000000)); macroscope.output("escaped.csv", "ESCAPED"); }
