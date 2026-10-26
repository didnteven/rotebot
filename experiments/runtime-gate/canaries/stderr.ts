const chunk = new Uint8Array(64 * 1024).fill(120);
while (true) Deno.stderr.writeSync(chunk);
