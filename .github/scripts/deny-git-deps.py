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

for crate in Workspace.from_path(root).crates:
	for dep in crate.dependencies:
		if dep.location != DependencyLocation.GIT:
			continue
		
		if crate.name in KNOWN_BAD_GIT_DEPS.get(dep.name, []):
			print(f'🤨 Ignoring bad git dependency {dep.name} of {crate.name}')
		else:
			print(f'🚫 Found git dependency {dep.name} of {crate.name}')
			sys.exit(1)				
