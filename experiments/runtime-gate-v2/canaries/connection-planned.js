function run(){if(macroscope.connection.request("gmail","create_draft",{body:"review me"}) !== undefined) throw "network data returned";}
