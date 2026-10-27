function run(){if(typeof Worker === "undefined"){macroscope.log("DENIED: Worker absent");return;}new Worker("data:text/javascript,for(;;){}");macroscope.output("escaped.csv","ESCAPED");}
