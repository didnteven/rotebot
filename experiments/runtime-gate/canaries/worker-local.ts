const worker = new Worker(new URL("./worker-body.ts", import.meta.url).href, { type: "module" });
worker.postMessage("read");
await new Promise((resolve, reject) => {
  worker.onmessage = (e) => resolve(e.data);
  worker.onerror = (e) => reject(new Error(e.message));
});
worker.terminate();
console.log(JSON.stringify({ escaped: true, canary: "worker-local" }));
Deno.exit(42);
