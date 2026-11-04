#!/usr/bin/env bash
set -euo pipefail

release_tag="${GITHUB_REF_NAME:?This script must run for a version tag}"
release_version="${release_tag#v}"

if [[ "$release_version" == "$release_tag" ]]; then
  echo "Expected a version tag beginning with 'v'; got '$release_tag'." >&2
  exit 1
fi

python3 - "$release_version" <<'PY'
import json
import subprocess
import sys

expected_version = sys.argv[1]
metadata = json.loads(
    subprocess.check_output(
        ["cargo", "metadata", "--locked", "--no-deps", "--format-version", "1"],
        text=True,
    )
)
packages = {package["name"]: package["version"] for package in metadata["packages"]}
release_crates = (
    "langmeld-core",
    "langmeld-expression",
    "langmeld-language",
    "langmeld",
    "langmeld-lsp",
    "langmeld-cli",
)

for name in release_crates:
    actual_version = packages.get(name)
    if actual_version is None:
        raise SystemExit(f"Release crate {name!r} is missing from workspace metadata.")
    if actual_version != expected_version:
        raise SystemExit(
            f"Tag version {expected_version} does not match {name} version {actual_version}."
        )

print(f"Validated all release crate versions against tag v{expected_version}.")
PY

if [[ "${1:-}" == "--check-only" ]]; then
  exit 0
fi

if [[ $# -gt 0 ]]; then
  echo "Usage: $0 [--check-only]" >&2
  exit 2
fi

release_crates=(
  langmeld-core
  langmeld-expression
  langmeld-language
  langmeld
  langmeld-lsp
  langmeld-cli
)

for crate in "${release_crates[@]}"; do
  if cargo info --registry crates-io "$crate@$release_version" >/dev/null 2>&1; then
    echo "$crate@$release_version is already published; skipping."
    continue
  fi

  echo "Publishing $crate@$release_version."
  cargo publish --locked -p "$crate"
done
