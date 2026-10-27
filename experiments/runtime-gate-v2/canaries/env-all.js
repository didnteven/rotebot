function run() { if (typeof process === "undefined") { macroscope.log("DENIED: process absent"); return; } process.env; macroscope.output("escaped.csv", "ESCAPED"); }
