function run(){for(const f of [function(){},async function(){},function*(){},async function*(){},macroscope.input])if(f.constructor!==undefined)throw 1;macroscope.log("DENIED: constructors absent");}
