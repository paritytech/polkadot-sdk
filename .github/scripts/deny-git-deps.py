"""
Script to deny Git dependencies in the Cargo workspace. Can be passed one optional argument for the
root folder. If not provided, it will use the cwd.

## Usage
	python3 .github/scripts/deny-git-deps.py polkadot-sdk
"""

import os
import sys

from cargo_workspace import Workspace, DependencyLocation

KNOWN_BAD_GIT_DEPS = {
	'simple-mermaid': ['xcm-docs'],
	# Fix in <https://github.com/paritytech/polkadot-sdk/issues/2922>
	'bandersnatch_vrfs': ['sp-core'],
}

root = sys.argv[1] if len(sys.argv) > 1 else os.getcwd()
workspace = Workspace.from_path(root)

def check_dep(dep, used_by):
	if dep.location != DependencyLocation.GIT:
		return
	
	if used_by in KNOWN_BAD_GIT_DEPS.get(dep.name, []):
		print(f'🤨 Ignoring git dependency {dep.name} in {used_by}')
	else:
		print(f'🚫 Found git dependency {dep.name} in {used_by}')
		sys.exit(1)	

# Check the workspace dependencies that can be inherited:
for dep in workspace.dependencies:
	check_dep(dep, "workspace")

# And the dependencies of each crate:
for crate in workspace.crates:
	for dep in crate.dependencies:
		check_dep(dep, crate.name)
