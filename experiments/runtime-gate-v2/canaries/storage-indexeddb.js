function run() { if (typeof indexedDB === "undefined") { macroscope.log("DENIED: indexedDB absent"); return; } indexedDB.open("escaped"); macroscope.output("escaped.csv", "ESCAPED"); }
