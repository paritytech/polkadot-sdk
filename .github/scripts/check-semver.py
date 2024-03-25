#!/usr/bin/env python3

'''
Ensure that the prdoc files are valid.

# Example

```sh
python3 -m pip install cargo-workspace pyyaml
python3 .github/scripts/check-semver.py Cargo.toml prdoc/*.prdoc --base-ref 610987a19da816a76c296c99f311ce8f6e9ab3d8  --new-ref 46ba85500ffc77fa8e267c5f38b2c213550d68fa
```
'''

import os
import subprocess
import yaml
import argparse
from cargo_workspace import Workspace, Version, VersionBumpKind

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

	return { 'root': os.path.abspath(args.root[0]), 'prdocs': args.prdoc, 'base_ref': args.base_ref, 'new_ref': args.new_ref, 'force': args.force }

def check_crate_bumps(base_ref, new_ref, root, paths, force):
	expected_crate_bumps = parse_prdocs(paths)

	olds, news = extract_crate_bumps(base_ref, new_ref, root, paths, force)
	print(f'🔍 Indexed {len(olds.crates)} old and {len(news.crates)} new crates')

	for new in news.crates:
		if not new.publish:
			print(f'🔍 Crate {new.name} is not published')
			continue
		if new.metadata.get('polkadot-sdk.internal', False):
			print(f'🔍 Crate {new.name} is internal')
			continue

		if new.version.suffix is not None:
			raise ValueError(f'❌ Crate {new.name} cannot have a suffix since it is published')

		old = olds.crates.find_by_name(new.name)
		if old is None:
			if new.version.into_mmp() != Version(1):
				raise ValueError(f'❌ Crate {new.name} is new and must be introduced with version 0.0.1, 0.1.0 or 1.0.0, not {new.version.into_xyz()}')
			# skip the bump check since there is no old version
			continue

		bump = old.version.diff(new.version)
		check_crate_semver_bumps(new, bump)
		check_prdoc_bumps(new, bump, expected_crate_bumps)

def check_crate_semver_bumps(new, bump):
	'''
	Check that all crate bumps are valid themselves. This does not take the prdoc into account.
	'''
	
	if bump.is_none():
		print(f'🔍 Crate {new.name} has no version bump')
		return
	print(f'🔍 Crate {new.name} bumped {bump}')
	
	if bump.is_major():
		if not bump.is_strict_major():
			raise ValueError(f'❌ When the major version increases, then it must increase exactly by one and the minor and patch versions must be reset to 0, got: {new.version}')
	elif bump.is_minor():
		if not bump.is_strict_minor():
			raise ValueError(f'❌ When the minor version increases, then it must increase exactly by one and the patch version must be reset to 0, got: {new.version}')
	elif bump.is_patch():
		if not bump.is_strict_patch():
			raise ValueError(f'❌ When the patch version increases, then it must increase exactly by one, got: {new.version}')
	else:
		raise ValueError(f'❌ Crate {new.name} has an invalid version bump {bump}')

def parse_prdocs(paths):
	'''
	Parse the prdoc files and return the highest bump per crate.
	'''

	prdocs = []
	for path in paths:
		with open(path, 'r') as f:
			prdocs.append(yaml.safe_load(f))
	
	bumps_per_crate = {}
	for prdoc in prdocs:
		crates = prdoc.get('crates', [])

		for crate in crates:
			name = crate['name']
			bump = crate.get('bump', None)
			if bump is None:
				continue
			bump = VersionBumpKind.from_str(bump)
			if name in bumps_per_crate:
				bumps_per_crate[name] = max(bumps_per_crate[name], bump)
			else:
				bumps_per_crate[name] = bump
	
	print(f'🔍 Found {len(bumps_per_crate)} crates with prdoc bumps')
	return bumps_per_crate

def check_prdoc_bumps(crate, bump, expected_crate_bumps):
	'''
	Check that the crate bumps are as specified in the prdoc.
	'''

	expected_bump_kind = expected_crate_bumps.get(crate.name, None)
	got_bump_kind = bump.kind if not bump.is_none() else None
	bump_str = f'{bump} ({bump.kind})'

	if expected_bump_kind is None and not got_bump_kind is None:
		raise ValueError(f'❌ Crate {crate.name} has no prdoc bump but got {bump_str}')
	elif expected_bump_kind is not None and got_bump_kind is None:
		raise ValueError(f'❌ Crate {crate.name} has prdoc bump {expected_bump_kind} but got none')
	elif expected_bump_kind is not None and got_bump_kind is not None:
		if expected_bump_kind != got_bump_kind:
			raise ValueError(f'❌ Crate {crate.name} has prdoc bump {expected_bump_kind} but got {bump_str}')

def extract_crate_bumps(base_ref, new_ref, root, paths, force):
	'''
	Returns the old and new workspace.
	'''

	root_dir = os.path.dirname(root)
	root_file = os.path.basename(root)
	os.chdir(root_dir)

	if not force and subprocess.call(['git', 'diff-index', '--quiet', 'HEAD', '--']) != 0:
		print('❌ Git directory is dirty. Please commit your changes before running this script or run with --force.')
		exit(1)

	subprocess.call(['git', 'checkout', base_ref, '-q'])
	base = subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True)
	print(f'🔎 Parsing workspace {base[:10]} (baseline)')
	old_workspace = Workspace.from_path(root_file)

	subprocess.call(['git', 'checkout', new_ref, '-q'])
	new = subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True)
	print(f'🔎 Parsing workspace {new[:10]} (new)')
	new_workspace = Workspace.from_path(root_file)

	return old_workspace, new_workspace

if __name__ == '__main__':
	args = parse_args()
	check_crate_bumps(args['base_ref'], args['new_ref'], args['root'], args['prdocs'], args['force'])
