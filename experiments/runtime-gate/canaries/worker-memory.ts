const workers: Worker[] = [];
const source = __MEMORY_SOURCE_JSON__;
for (let i = 0; i < 4; i++) {
  workers.push(new Worker("data:application/typescript," + encodeURIComponent(source), { type: "module" }));
}
setInterval(() => {}, 1000);
