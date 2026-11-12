#!/usr/bin/env bash
set -euo pipefail

usage() {
  echo "Usage: $0 [--check-only [vMAJOR.MINOR.PATCH[-PRERELEASE]]]" >&2
}

check_only=false
release_tag="${GITHUB_REF_NAME:-}"
case "${1:-}" in
  "") ;;
  --check-only)
    check_only=true
    if [[ $# -gt 2 ]]; then
      usage
      exit 2
    fi
    if [[ $# -eq 2 ]]; then
      release_tag="$2"
    fi
    ;;
  *)
    usage
    exit 2
    ;;
esac

if [[ ! "$release_tag" =~ ^v([0-9]+\.[0-9]+\.[0-9]+)(-[0-9A-Za-z.-]+)?$ ]]; then
  echo "Expected a version tag like v0.1.0; got '${release_tag:-<empty>}'" >&2
  exit 1
fi
release_version="${release_tag#v}"

# Keep dependency crates before the CLI, and keep this list aligned with the
# publishable workspace packages. The metadata check below catches drift.
release_crates=(
  langmeld-core
  langmeld-expression
  langmeld-language
  langmeld-project
  langmeld-build
  langmeld-lsp
  langmeld-textmate
  langmeld-tree-sitter
  langmeld-cli
)

python3 - "$release_version" "${release_crates[@]}" <<'PY'
import json
import subprocess
import sys

expected_version = sys.argv[1]
release_crates = sys.argv[2:]
metadata = json.loads(
    subprocess.check_output(
        ["cargo", "metadata", "--locked", "--no-deps", "--format-version", "1"],
        text=True,
    )
)
publishable = {
    package["name"]: package["version"]
    for package in metadata["packages"]
    if package["publish"] is None or "crates-io" in package["publish"]
}
expected = set(release_crates)
actual = set(publishable)
if actual != expected:
    missing = sorted(actual - expected)
    unknown = sorted(expected - actual)
    raise SystemExit(
        "Release package list is out of date; missing from order: "
        f"{missing}; absent or non-publishable: {unknown}."
    )

for name in release_crates:
    actual_version = publishable[name]
    if actual_version != expected_version:
        raise SystemExit(
            f"Tag version {expected_version} does not match "
            f"{name} version {actual_version}."
        )

print(f"Validated all {len(release_crates)} release crate versions against v{expected_version}.")
PY

if "$check_only"; then
  exit 0
fi

for crate in "${release_crates[@]}"; do
  if cargo info --registry crates-io "$crate@$release_version" >/dev/null 2>&1; then
    echo "$crate@$release_version is already published; skipping."
    continue
  fi

  echo "Publishing $crate@$release_version."
  cargo publish --locked -p "$crate"
done
