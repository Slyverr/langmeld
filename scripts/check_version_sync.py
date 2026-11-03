#!/usr/bin/env python3
"""Check workspace crate versions, internal requirements, and consumer locks."""

from __future__ import annotations

import argparse
import sys
import tomllib
from pathlib import Path


DEPENDENCY_SECTIONS = {"dependencies", "dev-dependencies", "build-dependencies"}


def read_toml(path: Path) -> dict:
    with path.open("rb") as file:
        return tomllib.load(file)


def find_dependency_tables(value: object):
    if not isinstance(value, dict):
        return
    for key, child in value.items():
        if key in DEPENDENCY_SECTIONS and isinstance(child, dict):
            yield child
        else:
            yield from find_dependency_tables(child)


def check(root: Path) -> list[str]:
    root_manifest = root / "Cargo.toml"
    root_data = read_toml(root_manifest)
    expected_version = root_data["workspace"]["package"]["version"]
    members = [
        root / member / "Cargo.toml"
        for member in root_data["workspace"]["members"]
    ]
    package_names: set[str] = set()
    errors: list[str] = []

    for manifest in members:
        data = read_toml(manifest)
        package = data["package"]
        name = package["name"]
        package_names.add(name)
        if package.get("version") != {"workspace": True}:
            errors.append(
                f"{manifest.relative_to(root)}: package version must inherit "
                "workspace.package.version"
            )

    manifest_paths = {root / "Cargo.toml"}
    for folder in (root / "crates", root / "examples", root / "benchmarks"):
        if folder.exists():
            manifest_paths.update(folder.rglob("Cargo.toml"))
    manifests = sorted(manifest_paths)
    for manifest in manifests:
        data = read_toml(manifest)
        for dependencies in find_dependency_tables(data):
            for alias, raw_spec in dependencies.items():
                spec = {"version": raw_spec} if isinstance(raw_spec, str) else raw_spec
                if not isinstance(spec, dict):
                    continue
                package_name = spec.get("package", alias)
                if package_name not in package_names:
                    continue
                version = spec.get("version")
                if version is None and spec.get("path"):
                    continue
                if version != expected_version:
                    relative = manifest.relative_to(root)
                    errors.append(
                        f"{relative}: dependency {alias} requires {version!r}; "
                        f"expected {expected_version!r}"
                    )

    lockfiles = [root / "Cargo.lock"]
    lockfiles.extend(
        path
        for folder in (root / "crates", root / "examples", root / "benchmarks")
        if folder.exists()
        for path in folder.rglob("Cargo.lock")
    )
    for lockfile in lockfiles:
        if not lockfile.is_file():
            continue
        lock_data = read_toml(lockfile)
        for package in lock_data.get("package", []):
            if (
                package["name"] in package_names
                and package["version"] != expected_version
            ):
                errors.append(
                    f"{lockfile.relative_to(root)}: locked {package['name']} "
                    f"{package['version']}; expected {expected_version}"
                )

    return errors


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--root",
        type=Path,
        default=Path(__file__).resolve().parents[1],
        help="Langmeld checkout to inspect (defaults to this script's repository)",
    )
    args = parser.parse_args()
    root = args.root.resolve()
    try:
        errors = check(root)
    except (OSError, KeyError, tomllib.TOMLDecodeError) as error:
        print(f"version check could not read the workspace: {error}", file=sys.stderr)
        return 2
    if errors:
        print("Langmeld version metadata is out of sync:", file=sys.stderr)
        for error in errors:
            print(f"- {error}", file=sys.stderr)
        return 1
    version = read_toml(root / "Cargo.toml")["workspace"]["package"]["version"]
    print(f"workspace and consumer versions align at {version}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
