function run(){if(typeof Worker === "undefined"){macroscope.log("DENIED: Worker absent");return;}new Worker("https://example.com/worker.js");macroscope.output("escaped.csv","ESCAPED");}
