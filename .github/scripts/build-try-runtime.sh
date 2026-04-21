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
# try-runtime-cli ships a rust-toolchain.toml that pins `channel = "stable"`.
# Building try-runtime-cli against polkadot-sdk only needs whatever stable is already
# installed in the image, so drop the pin and use the preinstalled toolchain as-is.
rm -f "$WORK_DIR/try-runtime-cli/rust-toolchain.toml" \
      "$WORK_DIR/try-runtime-cli/rust-toolchain"
echo "::endgroup::"

echo "::group::Generate Cargo patches"
# Parse the polkadot-sdk workspace Cargo.toml to extract all crates with path = "..."
# and generate [patch] entries so try-runtime-cli uses our local code.
# Uses the actual package name from each crate's Cargo.toml as the patch key,
# since some workspace aliases differ from the published crate name
# (e.g. workspace key "xcm" -> package name "staging-xcm").
python3 - "$SDK_PATH" "$WORK_DIR/try-runtime-cli/Cargo.toml" <<'PYEOF'
import re, sys, os

sdk_path = sys.argv[1]
target_cargo_toml = sys.argv[2]

with open(f"{sdk_path}/Cargo.toml") as f:
    content = f.read()

pattern = r'^(\S+)\s*=\s*\{[^}]*path\s*=\s*"([^"]+)"[^}]*\}'
matches = re.findall(pattern, content, re.MULTILINE)

# Extract `name = "..."` from the [package] section of a crate's Cargo.toml.
# Avoids the tomllib stdlib module (Python 3.11+) since older CI images may
# ship an earlier interpreter. Cargo does not allow inheriting `name` from
# the workspace, so the field is always a literal string.
pkg_section_re = re.compile(r'^\[package\]\s*\n(.*?)(?=^\[|\Z)', re.MULTILINE | re.DOTALL)
name_re = re.compile(r'^name\s*=\s*"([^"]+)"', re.MULTILINE)

def read_pkg_name(path):
    try:
        with open(path) as f:
            text = f.read()
    except OSError:
        return None
    section = pkg_section_re.search(text)
    if not section:
        return None
    m = name_re.search(section.group(1))
    return m.group(1) if m else None

patches = {}  # pkg_name -> abs_path (deduplicated by actual package name)
for _, rel_path in matches:
    crate_toml = os.path.join(sdk_path, rel_path, "Cargo.toml")
    if not os.path.isfile(crate_toml):
        continue
    pkg_name = read_pkg_name(crate_toml)
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
# Seed the lockfile from polkadot-sdk so yanked-but-already-locked registry
# versions in polkadot-sdk's transitive graph stay resolvable.
cp "$SDK_PATH/Cargo.lock" Cargo.lock
cargo build --release -p try-runtime-cli
cp target/release/try-runtime "$OUTPUT"
echo "::endgroup::"

echo "Built try-runtime at $OUTPUT"
"$OUTPUT" --version
