"""
Script to deny Git dependencies in the Cargo workspace. Can be passed one optional argument for the
root folder. If not provided, it will use the cwd.

## Usage
	python3 .github/scripts/deny-git-deps.py polkadot-sdk
"""

import os
import sys

from cargo_workspace import Workspace, DependencyLocation

# Some crates are allowed to have git dependencies until we fix them.
ALLOWED_GIT_DEPS = {
	'subwasmlib': ['polkadot-zombienet-sdk-tests'],
}

root = sys.argv[1] if len(sys.argv) > 1 else os.getcwd()
workspace = Workspace.from_path(root)
errors = []

def check_dep(dep, used_by):
	if dep.location != DependencyLocation.GIT:
		return

	if used_by in ALLOWED_GIT_DEPS.get(dep.name, []):
		print(f'🤨 Ignoring git dependency {dep.name} in {used_by}')
	else:
		errors.append(f'🚫 Found git dependency {dep.name} in {used_by}')

# Check the workspace dependencies that can be inherited:
for dep in workspace.dependencies:
	check_dep(dep, "workspace")

	if workspace.crates.find_by_name(dep.name):
		if dep.location != DependencyLocation.PATH:
			errors.append(f'🚫 Workspace must use path to link local dependency {dep.name}')

# And the dependencies of each crate:
for crate in workspace.crates:
	for dep in crate.dependencies:
		check_dep(dep, crate.name)

if errors:
	print('❌ Found errors:')
	for error in errors:
		print(error)
	sys.exit(1)
