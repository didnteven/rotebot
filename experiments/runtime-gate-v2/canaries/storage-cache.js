function run() { if (typeof caches === "undefined") { macroscope.log("DENIED: caches absent"); return; } caches.open("escaped"); macroscope.output("escaped.csv", "ESCAPED"); }
