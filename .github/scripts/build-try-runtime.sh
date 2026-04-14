#!/usr/bin/env bash
# Build try-runtime-cli from source with polkadot-sdk dependencies patched to use the local checkout.
#
# Usage: build-try-runtime.sh <polkadot-sdk-path> <try-runtime-version> <output-binary>
# Example: build-try-runtime.sh /workspace v0.10.1 ./try-runtime

set -eu -o pipefail

SDK_PATH="$(realpath "$1")"
VERSION="${2}"
OUTPUT="$(realpath -m "$3")"

WORK_DIR="$(mktemp -d)"
trap 'rm -rf "$WORK_DIR"' EXIT

echo "::group::Clone try-runtime-cli ${VERSION}"
git clone --depth 1 --branch "${VERSION}" \
    https://github.com/paritytech/try-runtime-cli.git "$WORK_DIR/try-runtime-cli"
echo "::endgroup::"

echo "::group::Generate Cargo patches"
# Parse the polkadot-sdk workspace Cargo.toml to extract all crates with path = "..."
# and generate [patch] entries so try-runtime-cli uses our local code.
# Uses the actual package name from each crate's Cargo.toml as the patch key,
# since some workspace aliases differ from the published crate name
# (e.g. workspace key "xcm" -> package name "staging-xcm").
python3 - "$SDK_PATH" "$WORK_DIR/try-runtime-cli/Cargo.toml" <<'PYEOF'
import re, sys, os, tomllib

sdk_path = sys.argv[1]
target_cargo_toml = sys.argv[2]

with open(f"{sdk_path}/Cargo.toml") as f:
    content = f.read()

pattern = r'^(\S+)\s*=\s*\{[^}]*path\s*=\s*"([^"]+)"[^}]*\}'
matches = re.findall(pattern, content, re.MULTILINE)

patches = {}  # pkg_name -> abs_path (deduplicated by actual package name)
for _, rel_path in matches:
    crate_toml = os.path.join(sdk_path, rel_path, "Cargo.toml")
    if not os.path.isfile(crate_toml):
        continue
    with open(crate_toml, "rb") as f:
        try:
            meta = tomllib.load(f)
        except Exception:
            continue
    pkg_name = meta.get("package", {}).get("name")
    if not pkg_name:
        continue
    patches[pkg_name] = f"{sdk_path}/{rel_path}"

patch_lines = ['\n[patch."https://github.com/paritytech/polkadot-sdk"]']
for name in sorted(patches):
    patch_lines.append(f'{name} = {{ path = "{patches[name]}" }}')

with open(target_cargo_toml, "a") as f:
    f.write("\n".join(patch_lines) + "\n")

print(f"Added {len(patches)} patch entries")
PYEOF
echo "::endgroup::"

echo "::group::Apply source compatibility patches"
# BackendRuntimeCode::new now takes a TryPendingCode argument.
# try-runtime doesn't use pending code, so we pass TryPendingCode::No.
sed -i 's/BackendRuntimeCode::new(\([^)]*\))/BackendRuntimeCode::new(\1, sp_state_machine::backend::TryPendingCode::No)/g' \
    "$WORK_DIR/try-runtime-cli/core/src/common/state.rs"
echo "Patched BackendRuntimeCode::new calls"
echo "::endgroup::"

echo "::group::Build try-runtime"
cd "$WORK_DIR/try-runtime-cli"
# Remove the lock file since patched dependencies will have different versions.
rm -f Cargo.lock
cargo build --release -p try-runtime-cli
cp target/release/try-runtime "$OUTPUT"
echo "::endgroup::"

echo "Built try-runtime at $OUTPUT"
"$OUTPUT" --version
