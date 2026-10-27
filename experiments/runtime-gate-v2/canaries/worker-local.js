function run(){if(typeof Worker === "undefined"){macroscope.log("DENIED: Worker absent");return;}new Worker("file:///synthetic/worker.js");macroscope.output("escaped.csv","ESCAPED");}
