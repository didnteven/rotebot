import { spawnSync } from "node:child_process";
import { mkdirSync, copyFileSync, chmodSync } from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";

if (process.platform !== "darwin") process.exit(0);
const desktop = fileURLToPath(new URL("../", import.meta.url));
const workspace = path.resolve(desktop, "../..");
const scratch = process.env.MACROSCOPE_SWIFT_SCRATCH ||
  "/Volumes/250GB SAMSUNG 970 EVO PLUS /dev-tools/macroscope-claude-targets/stage-c/swift";
const host = process.arch === "arm64" ? "aarch64-apple-darwin" : "x86_64-apple-darwin";
const target = process.env.TAURI_ENV_TARGET_TRIPLE || host;
const arch = target === "aarch64-apple-darwin" ? "arm64" : target === "x86_64-apple-darwin" ? "x86_64" : null;
if (!arch) throw new Error("Capture helper requires a macOS architecture build");
const configuration = process.env.TAURI_ENV_DEBUG === "true" ? "debug" : "release";
const args = ["--package-path", path.join(workspace, "native/macos-observer"),
  "--scratch-path", scratch, "--cache-path", path.join(scratch, "cache"),
  "--config-path", path.join(scratch, "config"), "--security-path", path.join(scratch, "security"),
  "--arch", arch, "-c", configuration];
if (process.env.MACROSCOPE_SWIFT_DISABLE_SANDBOX === "1") args.push("--disable-sandbox");
function run(extra) {
  const result = spawnSync("swift", ["build", ...args, ...extra], { cwd: workspace, encoding: "utf8",
    env: { ...process.env, CLANG_MODULE_CACHE_PATH: path.join(scratch, "ModuleCache"),
      SWIFTPM_MODULECACHE_OVERRIDE: path.join(scratch, "ModuleCache") } });
  if (result.status !== 0) throw new Error(result.stderr || "Swift capture helper build failed");
  return result.stdout.trim();
}
for (const product of ["macroscope-capture", "macroscope-bot-matte"]) {
  run(["--product", product]);
  const bin = run(["--show-bin-path"]);
  const destination = path.join(desktop, "src-tauri/binaries", `${product}-${target}`);
  mkdirSync(path.dirname(destination), { recursive: true });
  copyFileSync(path.join(bin, product), destination);
  chmodSync(destination, 0o755);
  // Preserve capture's permission identity; the image helper requests no screen access.
  const identifier = product === "macroscope-capture" ? "dev.macroscope.desktop.capture" : "dev.macroscope.desktop.bot-matte";
  const signed = spawnSync("codesign", ["--force", "--sign", "-", "--identifier", identifier, destination], { encoding: "utf8" });
  if (signed.status !== 0) throw new Error(signed.stderr || "Signing the native helper failed");
  console.log(`Prepared native helper: ${destination}`);
}
