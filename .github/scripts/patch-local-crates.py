#!/usr/bin/env python3
"""
Make crates.io dependents use the local copy of each crate that `parity-publish apply --registry`
keeps local.

`parity-publish apply --registry` keeps bumped crates as path dependencies and takes all other
crates from crates.io. A crates.io crate then depends on the crates.io copy of a bumped crate,
and Cargo treats it as a different package than the local copy. A local crate that uses both gets
two copies and fails with "mismatched types". After a real publish, semver-compatible bumps
resolve to one copy.

This script adds a `[patch.crates-io]` entry for each workspace dependency that has both `path`
and `version`, and removes the crates.io copies of these crates from `Cargo.lock`. Cargo ignores
the patch for a crate with a major bump, so real breaking changes still fail to compile.

Usage: patch-local-crates.py <workspace root>
"""

import subprocess
import sys
import tomllib
from pathlib import Path


def main():
	root = Path(sys.argv[1] if len(sys.argv) > 1 else ".")
	manifest = root / "Cargo.toml"

	with open(manifest, "rb") as f:
		workspace = tomllib.load(f)

	if "patch" in workspace and "crates-io" in workspace["patch"]:
		sys.exit("Cargo.toml already has a [patch.crates-io] section")

	local = {}
	for name, dep in workspace["workspace"]["dependencies"].items():
		if isinstance(dep, dict) and "path" in dep and "version" in dep:
			local[dep.get("package", name)] = dep["path"]

	with open(manifest, "a") as f:
		f.write("\n[patch.crates-io]\n")
		for name, path in sorted(local.items()):
			f.write(f'{name} = {{ path = "{path}" }}\n')
	print(f"Patched {len(local)} crates")

	lockfile = root / "Cargo.lock"
	if not lockfile.exists():
		return

	with open(lockfile, "rb") as f:
		lock = tomllib.load(f)

	for package in lock.get("package", []):
		if package["name"] in local and package.get("source", "").startswith("registry+"):
			spec = f'{package["name"]}@{package["version"]}'
			print(f"Removing the crates.io copy {spec} from Cargo.lock")
			subprocess.run(["cargo", "update", "-p", spec], cwd=root, check=True)


if __name__ == "__main__":
	main()
