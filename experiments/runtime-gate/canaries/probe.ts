// Synthetic effects only. A denial is accepted only for a permission/security
// error, never an arbitrary JS exception or a failed network connection.
export async function probe(name: string, effect: () => unknown | Promise<unknown>) {
  try {
    await effect();
    console.log(JSON.stringify({ escaped: true, canary: name }));
    Deno.exit(42);
  } catch (error) {
    const e = error as Error;
    if (/NotCapable|PermissionDenied|SecurityError/.test(e.name) ||
        (name.startsWith("storage-") && e.name === "QuotaExceededError") ||
        (name.endsWith("remote") && e.message.includes("--no-remote")) ||
        /Requires .* access|not allowed|permission denied/i.test(e.message)) {
      console.log(JSON.stringify({ denied: true, canary: name, error: e.name, message: e.message.slice(0, 160) }));
      console.error(`${e.name}: ${e.message}`.slice(0, 200));
    } else {
      console.error(`UNEXPECTED ${e.name}: ${e.message}`);
      Deno.exit(43);
    }
  }
}
