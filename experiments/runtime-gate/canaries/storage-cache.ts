import { probe } from "./probe.ts";
if (!("caches" in globalThis)) {
  console.log(JSON.stringify({ unavailable: true, canary: "storage-cache", api: "Cache API" }));
} else {
  await probe("storage-cache", async () => {
    const cache = await caches.open("runtime-gate");
    for (let i = 0; i < 64; i++) {
      await cache.put(`https://example.com/${i}`, new Response(new Uint8Array(1024 * 1024)));
    }
  });
}
