self.onmessage = async ({ data: action }) => {
  const effects: Record<string, () => unknown> = {
    read: () => Deno.readTextFile("/etc/hosts"),
    fetch: () => fetch("https://example.com"),
    connect: () => Deno.connect({ hostname: "127.0.0.1", port: 9 }),
    env: () => Deno.env.toObject(),
    command: () => new Deno.Command("/bin/sh", { args: ["-c", "exit 0"] }).output(),
    ffi: () => Deno.dlopen("/usr/lib/libSystem.B.dylib", {}),
    data: () => import("data:text/javascript,export default 123"),
    remote: () => import("https://example.com/runtime-gate-canary.ts"),
    write: () => Deno.writeTextFile("__OUTSIDE_WRITE__", "worker escape"),
    storage: () => { const s = "x".repeat(1024 * 1024); for (let i = 0; i < 64; i++) localStorage.setItem(`${i}`, s); },
  };
  try {
    await effects[action]();
    self.postMessage({ escaped: true, canary: `worker-${action}` });
  } catch (e) {
    const error = e as Error;
    const denied = /NotCapable|PermissionDenied|SecurityError/.test(error.name) ||
      (action === "storage" && error.name === "QuotaExceededError") ||
      (action === "remote" && error.message.includes("--no-remote")) ||
      /Requires .* access|not allowed|permission denied/i.test(error.message);
    console.error(`${error.name}: ${error.message}`.slice(0, 200));
    self.postMessage({ denied, unexpected: !denied, canary: `worker-${action}`, error: error.name, message: error.message.slice(0, 160) });
  }
};
