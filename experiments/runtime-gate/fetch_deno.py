"""Download official assets; never execute bytes before both digest checks pass."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile
import zipfile

ROOT = Path(__file__).resolve().parent
VERSION = "2.9.8"
ASSET = "deno-aarch64-apple-darwin.zip"
URL = f"https://github.com/denoland/deno/releases/download/v{VERSION}/"


def digest(path):
    h = hashlib.sha256()
    with path.open("rb") as f:
        for block in iter(lambda: f.read(1024 * 1024), b""):
            h.update(block)
    return h.hexdigest()


def download(name, target):
    subprocess.run(["/usr/bin/curl", "--fail", "--location", "--retry", "3",
                    "--silent", "--show-error", URL + name, "-o", str(target)], check=True)


def main():
    runtime = ROOT / ".runtime"
    runtime.mkdir(exist_ok=True)
    lock_path = ROOT / "runtime.lock.json"
    old = json.loads(lock_path.read_text()) if lock_path.exists() else None
    if old and old["version"] != VERSION:
        raise SystemExit("Version differs from committed lock; review pin update explicitly")
    with tempfile.TemporaryDirectory(dir=runtime) as tmp:
        tmp = Path(tmp)
        # Refetch published checksums even when reusing a cached archive.
        download(ASSET + ".sha256sum", tmp / "archive.sha256sum")
        download("deno-aarch64-apple-darwin.sha256sum", tmp / "binary.sha256sum")
        archive_sha = (tmp / "archive.sha256sum").read_text().split()[0].lower()
        binary_sha = (tmp / "binary.sha256sum").read_text().split()[0].lower()
        for value in (archive_sha, binary_sha):
            if len(value) != 64 or any(c not in "0123456789abcdef" for c in value):
                raise SystemExit("Invalid published checksum")
        if old and (old["archive_sha256"] != archive_sha or old["binary_sha256"] != binary_sha):
            raise SystemExit("Published SHA-256 differs from lock; refusing download")
        archive = runtime / ASSET
        if not archive.exists():
            download(ASSET, tmp / ASSET)
            if digest(tmp / ASSET) != archive_sha:
                raise SystemExit("Archive SHA-256 mismatch")
            os.replace(tmp / ASSET, archive)
        if digest(archive) != archive_sha:
            raise SystemExit("Cached archive SHA-256 mismatch (remove it deliberately to retry)")
        with zipfile.ZipFile(archive) as z:
            if z.namelist() != ["deno"]:
                raise SystemExit("Unexpected release archive members")
            (tmp / "deno").write_bytes(z.read("deno"))
        if digest(tmp / "deno") != binary_sha:
            raise SystemExit("Binary SHA-256 mismatch")
        binary = runtime / "deno"
        if binary.exists() and digest(binary) != binary_sha:
            raise SystemExit("Installed binary SHA-256 mismatch")
        if not binary.exists():
            (tmp / "deno").chmod(0o555)
            os.replace(tmp / "deno", binary)
        lock = dict(version=VERSION, target="aarch64-apple-darwin", archive_url=URL + ASSET,
                    archive_checksum_url=URL + ASSET + ".sha256sum",
                    binary_checksum_url=URL + "deno-aarch64-apple-darwin.sha256sum",
                    archive_sha256=archive_sha, binary_sha256=binary_sha)
        if not old:
            lock_path.write_text(json.dumps(lock, indent=2) + "\n")
    print(f"PASS Deno {VERSION}: archive SHA-256 {archive_sha}; binary SHA-256 {binary_sha}")


if __name__ == "__main__":
    main()
