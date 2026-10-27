function run() { if (typeof Deno === "undefined") { macroscope.log("DENIED: Deno absent"); return; } Deno.dlopen("/usr/lib/libSystem.B.dylib",{}); macroscope.output("escaped.csv", "ESCAPED"); }
