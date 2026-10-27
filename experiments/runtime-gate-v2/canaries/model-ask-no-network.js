function run() {
  if (typeof fetch !== "undefined" || typeof WebSocket !== "undefined" || typeof XMLHttpRequest !== "undefined" || typeof socket !== "undefined" || typeof Deno !== "undefined" || typeof process !== "undefined") throw new Error("network API exists");
  macroscope.log("DENIED: no fetch or socket API exists");
}
