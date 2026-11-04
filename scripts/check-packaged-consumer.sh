#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
target_dir="${CARGO_TARGET_DIR:-$repo_root/target}"
CARGO_TARGET_DIR="$(python3 -c 'import sys; from pathlib import Path; print(Path(sys.argv[1]).resolve())' "$target_dir")"
export CARGO_TARGET_DIR
package_root="${LANGMELD_PACKAGE_ROOT:-${CARGO_TARGET_DIR:-$repo_root/target}/package}"
package_root="$(python3 -c 'import sys; from pathlib import Path; print(Path(sys.argv[1]).resolve())' "$package_root")"
package_version="$(cargo metadata --locked --offline --no-deps --format-version 1 \
    --manifest-path "$repo_root/Cargo.toml" \
    | python3 -c 'import json, sys; print(next(package["version"] for package in json.load(sys.stdin)["packages"] if package["name"] == "langmeld"))')"
for package in langmeld langmeld-core langmeld-expression langmeld-language; do
    if [[ ! -f "$package_root/$package-$package_version/Cargo.toml" ]]; then
        echo "missing packaged source for $package; run the workspace cargo package check first" >&2
        exit 1
    fi
done

project="${LANGMELD_PACKAGE_CONSUMER_PROJECT:-$repo_root/../langmeld-packaged-consumer}"
project="$(python3 -c 'import sys; from pathlib import Path; print(Path(sys.argv[1]).resolve())' "$project")"
marker="$project/.langmeld-packaged-consumer"
base_manifest="$project/.langmeld-packaged-consumer-base.Cargo.toml"

if [[ ! -f "$marker" ]]; then
    if [[ -e "$project" && ( ! -d "$project" || -n "$(find "$project" -mindepth 1 -maxdepth 1 -print -quit)" ) ]]; then
        echo "refusing to reuse a nonempty or unmarked path at $project" >&2
        echo "set LANGMELD_PACKAGE_CONSUMER_PROJECT to a dedicated smoke-project path" >&2
        exit 1
    fi
    mkdir -p "$project"
    cargo run --locked --offline --manifest-path "$repo_root/Cargo.toml" -p langmeld-cli -- \
        init --path "$project" --name PackagedConsumer --no-interactive \
        --langmeld-path "$repo_root"
    cp "$project/Cargo.toml" "$base_manifest"
    printf '%s\n' "Langmeld packaged-consumer smoke project" > "$marker"
elif [[ ! -f "$base_manifest" ]]; then
    echo "marked consumer project is missing its base manifest: $base_manifest" >&2
    exit 1
fi

# Restore the generated manifest before patching local package paths, making
# repeated runs safe without deleting or recreating the consumer project.
cp "$base_manifest" "$project/Cargo.toml"

python3 - "$project/Cargo.toml" "$repo_root" "$project" "$package_root" \
    "$package_version" <<'PY'
import json
import os
import sys
from pathlib import Path

manifest_path, repo_root, project_root, package_root = map(Path, sys.argv[1:5])
package_version = sys.argv[5]
manifest = manifest_path.read_text()
workspace_facade = os.path.relpath(repo_root / "crates/langmeld", project_root)
old_path = f'path = "{workspace_facade}"'
if manifest.count(old_path) != 2:
    raise SystemExit("generated manifest did not contain both workspace facade dependencies")

packaged_facade = package_root / f"langmeld-{package_version}"
manifest = manifest.replace(old_path, f"path = {json.dumps(str(packaged_facade))}")
patches = [
    ("langmeld-core", "langmeld-core"),
    ("langmeld-expression", "langmeld-expression"),
    ("langmeld-language", "langmeld-language"),
]
manifest += "\n[patch.crates-io]\n"
for package, directory in patches:
    path = package_root / f"{directory}-{package_version}"
    manifest += f"{package} = {{ path = {json.dumps(str(path))} }}\n"
manifest_path.write_text(manifest)
PY

export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$repo_root/target}"
cargo test --offline --manifest-path "$project/Cargo.toml"
(cd "$project" && cargo run --locked --offline \
    --manifest-path "$project/Cargo.toml" --bin packagedconsumer-host -- check --project)
cargo run --locked --offline --manifest-path "$repo_root/Cargo.toml" -p langmeld-cli -- \
    generate --project-root "$project" --check
