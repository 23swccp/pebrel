"""Collect and validate source-bound notices for the Linux relay binaries."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess

from cargo_notices import collect


TARGETS = {"x86_64": "x86_64-unknown-linux-musl", "aarch64": "aarch64-unknown-linux-musl"}
MAX_NOTICE_BYTES = 16 * 1024 * 1024


def dependency_packages(metadata: dict) -> list[dict]:
    root = next(p for p in metadata["packages"] if p["name"] == "pebrel-mobile-link")
    nodes = {node["id"]: node for node in metadata["resolve"]["nodes"]}
    pending, reached = [root["id"]], set()
    while pending:
        package = pending.pop()
        if package in reached:
            continue
        reached.add(package)
        pending.extend(dependency["pkg"] for dependency in nodes[package]["deps"]
                       if any(kind["kind"] != "dev" for kind in dependency["dep_kinds"]))
    return [p for p in metadata["packages"] if p["id"] in reached and p["source"] is not None]


def _file_names(packages: list[dict]) -> set[str]:
    if not packages:
        raise ValueError("Missing native dependency notices")
    names = {"Cargo.lock", "LICENSE-SOURCE.json"}
    for package in packages:
        if not package.get("texts"):
            raise ValueError("Dependency notice has no license text")
        for text in package["texts"]:
            name = f"{package['name']}-{package['version']}/{text}"
            if "\\" in name or name.startswith("/") or any(part in ("", ".", "..") for part in name.split("/")):
                raise ValueError("Invalid dependency notice path")
            names.add(name)
    return names


def bounded_read(stream) -> bytes:
    data = stream.read(MAX_NOTICE_BYTES + 1)
    if len(data) > MAX_NOTICE_BYTES:
        raise ValueError("Dependency notice exceeds the size limit")
    return data


def directory_reader(directory: Path):
    root = directory.resolve(strict=True)

    def read(name: str) -> bytes:
        path = root / name
        if not path.resolve(strict=True).is_relative_to(root):
            raise ValueError("Dependency notice escapes its bundle")
        with path.open("rb") as stream:
            return bounded_read(stream)
    return read


def read_bundle(read, commit: str, arch: str) -> dict[str, bytes]:
    raw = read("manifest.json")
    manifest = json.loads(raw)
    if (manifest.get("schema_version") != 1 or manifest.get("commit") != commit or
            manifest.get("target") != TARGETS[arch]):
        raise ValueError("Dependency notices differ from relay source or target")
    names = _file_names(manifest["packages"])
    if set(manifest["files"]) != names:
        raise ValueError("Dependency notice inventory is incomplete")
    result, total = {"manifest.json": raw}, len(raw)
    for name in sorted(names):
        data = read(name)
        total += len(data)
        if total > MAX_NOTICE_BYTES or hashlib.sha256(data).hexdigest() != manifest["files"][name]:
            raise ValueError("Dependency notice is damaged or oversized")
        result[name] = data
    return result


def package(source: Path, output: Path, commit: str, arch: str) -> None:
    source = source.resolve(strict=True)
    if not re.fullmatch(r"[0-9a-f]{40}", commit):
        raise ValueError("Expected exact source commit")
    head = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=source, text=True, encoding="utf-8").strip()
    if head != commit:
        raise ValueError("Notice source checkout differs from the relay binary")
    subprocess.run(["git", "diff", "--exit-code", commit, "--", "Cargo.toml", "Cargo.lock", "mobile/link"],
                   cwd=source, stdout=subprocess.DEVNULL, check=True)
    target = TARGETS[arch]
    # Cargo 已完成目标筛选；只遍历该组件的非开发依赖，不按手写库名清单猜测许可。
    metadata = json.loads(subprocess.check_output([
        "cargo", "metadata", "--locked", "--offline", "--format-version", "1",
        "--manifest-path", str(source / "mobile/link/Cargo.toml"), "--features", "relay",
        "--filter-platform", target,
    ], cwd=source, text=True, encoding="utf-8"))
    license_source = source / "mobile/ssh/licenses/SOURCE.json"
    fallback = json.loads(license_source.read_text(encoding="utf-8"))
    packages = collect(dependency_packages(metadata), output,
                       license_source.parent / fallback["file"], fallback["sha256"])
    (output / "Cargo.lock").write_bytes((source / "Cargo.lock").read_bytes())
    (output / "LICENSE-SOURCE.json").write_bytes(license_source.read_bytes())
    files = {name: hashlib.sha256((output / name).read_bytes()).hexdigest() for name in sorted(_file_names(packages))}
    (output / "manifest.json").write_text(json.dumps({
        "schema_version": 1, "commit": commit, "target": target, "packages": packages, "files": files,
    }, indent=2) + "\n", encoding="utf-8", newline="\n")
    read_bundle(directory_reader(output), commit, arch)
    print(f"Relay notices: {len(packages)} resolved dependencies for {target}, source {commit}")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--commit", required=True)
    parser.add_argument("--arch", choices=TARGETS, required=True)
    args = parser.parse_args()
    package(args.source, args.output, args.commit, args.arch)
