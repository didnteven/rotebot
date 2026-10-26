// Enough attempted content to exceed the entire 32 MiB scratch budget.
import { probe } from "./probe.ts";
await probe("storage-local", () => {
  const chunk = "x".repeat(1024 * 1024);
  for (let i = 0; i < 64; i++) localStorage.setItem(`fill-${i}`, chunk);
});
