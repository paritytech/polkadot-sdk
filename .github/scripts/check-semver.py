#!/usr/bin/env python3

'''
Ensure that the prdoc files are valid.

# Example

```sh
python3 -m pip install cargo-workspace
python3 .github/scripts/check-prdoc.py Cargo.toml prdoc/*.prdoc
```

Produces example output:
```pre
🔎 Reading workspace polkadot-sdk/Cargo.toml
📦 Checking 32 prdocs against 493 crates.
✅ All prdocs are valid
```
'''

import os
import subprocess
import yaml
import argparse
import cargo_workspace

def parse_args():
	parser = argparse.ArgumentParser(description='Check prdoc files')
	
	parser.add_argument('root', help='The cargo workspace manifest', metavar='root', type=str, nargs=1)
	parser.add_argument('prdoc', help='The prdoc files', metavar='prdoc', type=str, nargs='*')
	parser.add_argument('--new-ref', help='The new ref that we should compare with.', required=True)
	parser.add_argument('--base-ref', help='The ancestor ref that we should compare with.', required=True)
	parser.add_argument('--force', help='Ignore dirty git directory.', action='store_true', required=False, default=False)
	args = parser.parse_args()

	if len(args.prdoc) == 0:
		print('❌ Need at least one prdoc file as argument.')
		exit(1)

	return { 'root': os.path.abspath(args.root[0]), 'prdocs': args.prdoc, 'base_ref': args.base_ref, new_ref: args.new_ref, 'force': args.force }

def check_crate_semver_bumps(base_ref, new_ref, root, paths, force):
	'''
	Check that all crates have their hightest necessary version bump mentioned in at least oneprdoc.
	'''

	if not force and subprocess.call(['git', 'diff-index', '--quiet', 'HEAD', '--']) != 0:
		print('❌ Git directory is dirty. Please commit your changes before running this script or run with --force.')
		exit(1)

	subprocess.call(['git', 'checkout', base_ref])
	base = subprocess.check_output(['git', 'rev-parse', 'HEAD']).strip().decode('utf-8')
	print(f'🔎 Parsing workspace {base[:10]} (baseline)')
	old_workspace = cargo_workspace.Workspace.from_path(root)

	subprocess.call(['git', 'checkout', new_ref])
	new = subprocess.check_output(['git', 'rev-parse', 'HEAD']).strip().decode('utf-8')
	print(f'🔎 Parsing workspace {new[:10]} (new)')
	new_workspace = cargo_workspace.Wgorkspace.from_path(root)

if __name__ == '__main__':
	args = parse_args()
	check_crate_semver_bumps(args['base_ref'], args['new_ref'], args['root'], args['prdocs'], args['force'])
