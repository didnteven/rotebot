import { spawnSync } from "node:child_process";
import { chmodSync, copyFileSync, mkdirSync } from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";

const desktop = fileURLToPath(new URL("../", import.meta.url));
const workspace = path.resolve(desktop, "../..");

function run(command, args) {
  const result = spawnSync(command, args, {
    cwd: workspace,
    encoding: "utf8",
    stdio: ["ignore", "pipe", "inherit"],
    maxBuffer: 16 * 1024 * 1024,
  });
  if (result.error) throw result.error;
  if (result.status !== 0) {
    process.stderr.write(result.stdout);
    process.exit(result.status ?? 1);
  }
  return result.stdout;
}

const host = run("rustc", ["-vV"]).match(/^host: (.+)$/m)?.[1];
if (!host) throw new Error("rustc did not report a host target triple");
const target = process.env.TAURI_ENV_TARGET_TRIPLE || host;
const args = ["build", "-p", "macroscope-runner", "--bins"];
if (target !== host) args.push("--target", target);
if (process.env.TAURI_ENV_DEBUG !== "true") args.push("--release");
args.push("--message-format=json-render-diagnostics");

// Cargo reports the actual artifact path, including CARGO_TARGET_DIR, target
// and profile settings. Never assume that the workspace has a target/ folder.
const messages = run("cargo", args).trim().split("\n").filter(Boolean).map(JSON.parse);
for (const message of messages) {
  if (message.reason === "compiler-message" && message.message.rendered) {
    process.stderr.write(message.message.rendered);
  }
}
const executable = messages.find(
  (message) => message.reason === "compiler-artifact"
    && message.target.name === "macroscope-runner"
    && message.executable,
)?.executable;
if (!executable) throw new Error("Cargo did not report the sandbox runner executable");

const destination = path.join(desktop, "src-tauri/binaries", `macroscope-runner-${target}`);
mkdirSync(path.dirname(destination), { recursive: true });
copyFileSync(executable, destination);
chmodSync(destination, 0o755);
console.log(`Prepared sandbox runner: ${executable} -> ${destination}`);
