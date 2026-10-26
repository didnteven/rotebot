// Each worker probe is separate so an early denial cannot mask later effects.
const action = "__WORKER_ACTION__";
// Local-file Worker bootstraps need read permission in this pin. A data URL
// bootstrap probes worker effects without granting any permission.
const source = __WORKER_SOURCE_JSON__;
const worker = new Worker("data:application/typescript," + encodeURIComponent(source), { type: "module" });
worker.postMessage(action);
const result = await new Promise((resolve, reject) => {
  worker.onmessage = (event) => resolve(event.data);
  worker.onerror = (event) => reject(new Error(event.message));
});
worker.terminate();
console.log(JSON.stringify(result));
if ((result as { escaped?: boolean }).escaped) Deno.exit(42);
