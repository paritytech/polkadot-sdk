"""
Script to deny Git dependencies in the Cargo workspace.

## Usage
	python3 .github/scripts/deny-git-deps.py polkadot-sdk
"""

import argparse
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

for crate in workspace.crates:
	for dep in crate.dependencies:
		if dep.location != DependencyLocation.GIT:
			continue
		
		if crate.name in KNOWN_BAD_GIT_DEPS.get(dep.name, []):
			print(f'🤨 Ignoring bad git dependency {dep.name} of {crate.name}')
		else:
			print(f'🚫 Found git dependency {dep.name} of {crate.name}')
			sys.exit(1)				
