import { probe } from "./probe.ts";
if (!("indexedDB" in globalThis)) {
  console.log(JSON.stringify({ unavailable: true, canary: "storage-indexeddb", api: "IndexedDB" }));
} else {
  await probe("storage-indexeddb", () => new Promise<void>((resolve, reject) => {
    const open = indexedDB.open("runtime-gate", 1);
    open.onerror = () => reject(open.error);
    open.onupgradeneeded = () => open.result.createObjectStore("fill");
    open.onsuccess = () => {
      const tx = open.result.transaction("fill", "readwrite");
      for (let i = 0; i < 64; i++) tx.objectStore("fill").put(new Uint8Array(1024 * 1024), i);
      tx.oncomplete = () => resolve();
      tx.onerror = () => reject(tx.error);
    };
  }));
}
