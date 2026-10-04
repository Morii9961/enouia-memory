"""Regenerate or check the Runtime integration surface manifest (ADR-MEM-45).

Enouia Runtime embeds the workspace Core at a pinned revision and hosts
Memory's local frontend. This script records every file of that surface with
its digest in docs/integration/runtime-surface.json. The Rust test
crates/enouia-memory-contract/tests/runtime_surface.rs recomputes the same
digests and requires the aggregate to appear in docs/integration/RUNTIME.md.

    python tools/integration/runtime_surface.py           # check, print aggregate
    python tools/integration/runtime_surface.py --write   # rewrite the manifest

Digest: SHA-256 over the file bytes with CRLF normalized to LF.
Aggregate: SHA-256 over "<path>\\t<sha256>\\n" lines sorted by path.
Standard library only; reads nothing outside this repository.
"""

import hashlib
import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
MANIFEST = ROOT / "docs/integration/runtime-surface.json"

# Individual files and the reason Runtime depends on them.
FILES = {
    # Wire contract between the page and the Core.
    "contracts/ipc/workspace-v1.schema.json": "wire",
    "contracts/memory/common-v1.schema.json": "wire",
    "tests/fixtures/memory/workspace-manifest.json": "wire",
    "crates/enouia-memory-contract/src/workspace.rs": "wire",
    "crates/enouia-memory-contract/src/error.rs": "wire",
    # Build closure Runtime resolves at the pinned revision.
    "Cargo.toml": "build",
    "rust-toolchain.toml": "build",
    "crates/enouia-memory-contract/Cargo.toml": "build",
    "crates/enouia-memory-vault/Cargo.toml": "build",
    "crates/enouia-memory-import/Cargo.toml": "build",
    "crates/enouia-memory-govern/Cargo.toml": "build",
    "crates/enouia-memory-index/Cargo.toml": "build",
    "crates/enouia-memory-context/Cargo.toml": "build",
    "crates/enouia-memory-workspace/Cargo.toml": "build",
    # Reference shell: duties Runtime's adapter mirrors.
    "apps/workspace/src-tauri/Cargo.toml": "reference-shell",
    "apps/workspace/src-tauri/build.rs": "reference-shell",
    "apps/workspace/src-tauri/tauri.conf.json": "reference-shell",
    # Reference frontend: behavior Runtime's Memory surfaces mirror.
    "apps/workspace/package.json": "reference-ui",
}

# Directories recorded completely: a new file there is a surface change.
DIRECTORIES = {
    "crates/enouia-memory-workspace/src": "core",
    "apps/workspace/src-tauri/src": "reference-shell",
    "apps/workspace/src-tauri/capabilities": "reference-shell",
    "apps/workspace/src": "reference-ui",
}


def digest(rel: str) -> str:
    data = (ROOT / rel).read_bytes().replace(b"\r\n", b"\n")
    return hashlib.sha256(data).hexdigest()


def commands() -> list:
    text = (ROOT / "crates/enouia-memory-contract/src/workspace.rs").read_text(encoding="utf-8")
    block = re.search(r"pub const COMMANDS: \[&str; \d+\] = \[(.*?)\];", text, re.S)
    return re.findall(r'"([a-z_]+)"', block.group(1))


def entries() -> list:
    found = dict(FILES)
    for directory, role in DIRECTORIES.items():
        for path in sorted((ROOT / directory).rglob("*")):
            if path.is_file():
                found[path.relative_to(ROOT).as_posix()] = role
    return [
        {"path": path, "role": role, "sha256": digest(path)}
        for path, role in sorted(found.items())
    ]


def aggregate(files: list) -> str:
    text = "".join(f"{f['path']}\t{f['sha256']}\n" for f in files)
    return hashlib.sha256(text.encode("utf-8")).hexdigest()


def build() -> dict:
    files = entries()
    return {
        "schemaVersion": 1,
        "description": (
            "Enouia Memory's Runtime integration surface (ADR-MEM-45): the files Enouia "
            "Runtime's Windows client depends on when it embeds enouia-memory-workspace at a "
            "pinned revision and hosts Memory's local frontend. Regenerate with "
            "tools/integration/runtime_surface.py and log the new aggregate in "
            "docs/integration/RUNTIME.md."
        ),
        "digest": "sha256 of file bytes with CRLF normalized to LF",
        "aggregateDigest": "sha256 of '<path>\\t<sha256>\\n' lines sorted by path",
        "commands": commands(),
        "coveredDirectories": sorted(DIRECTORIES),
        "files": files,
        "aggregate": aggregate(files),
    }


def main() -> int:
    current = build()
    if "--write" in sys.argv[1:]:
        MANIFEST.parent.mkdir(parents=True, exist_ok=True)
        MANIFEST.write_text(json.dumps(current, indent=2) + "\n", encoding="utf-8", newline="\n")
        print(f"wrote {MANIFEST.relative_to(ROOT).as_posix()}")
        print(f"aggregate {current['aggregate']}")
        return 0
    if not MANIFEST.exists():
        print("manifest missing; run with --write")
        return 1
    recorded = json.loads(MANIFEST.read_text(encoding="utf-8"))
    old = {f["path"]: f["sha256"] for f in recorded.get("files", [])}
    new = {f["path"]: f["sha256"] for f in current["files"]}
    changed = sorted(p for p in new.keys() | old.keys() if old.get(p) != new.get(p))
    for path in changed:
        print(f"changed {path}")
    log = (ROOT / "docs/integration/RUNTIME.md").read_text(encoding="utf-8")
    logged = current["aggregate"] in log
    print(f"aggregate {current['aggregate']} ({'logged' if logged else 'NOT logged'})")
    same = not changed and recorded.get("aggregate") == current["aggregate"]
    return 0 if same and logged and recorded.get("commands") == current["commands"] else 1


if __name__ == "__main__":
    sys.exit(main())
