#!/usr/bin/env python3

# Ensures that:
# - all crates are added to the root workspace
# - local dependencies are resolved via `path`
#
# It does not check that the local paths resolve to the correct crate. This is already done by cargo.
#
# Must be called with a folder containing a `Cargo.toml` workspace file.

import os
import sys
import toml
import argparse

def parse_args():
	parser = argparse.ArgumentParser(description='Check Rust workspace integrity.')

	parser.add_argument('workspace_dir', help='The directory to check', metavar='workspace_dir', type=str, nargs=1)
	parser.add_argument('--exclude', help='Exclude crate paths from the check', metavar='exclude', type=str, nargs='*', default=[])
	
	args = parser.parse_args()
	return (args.workspace_dir[0], args.exclude)

def main(root, exclude):
	workspace_crates = get_members(root, exclude)
	all_crates = get_crates(root, exclude)
	print(f'📦 Found {len(all_crates)} crates in total')
	
	check_duplicates(workspace_crates)
	check_missing(workspace_crates, all_crates)
	check_links(all_crates)

# Extract all members from a workspace.
# Return: list of all workspace paths
def get_members(workspace_dir, exclude):
	print(f'🔎 Indexing workspace {os.path.abspath(workspace_dir)}')

	root_manifest_path = os.path.join(workspace_dir, "Cargo.toml")
	if not os.path.exists(root_manifest_path):
		print(f'❌ No root manifest found at {root_manifest}')
		sys.exit(1)

	root_manifest = toml.load(root_manifest_path)
	if not 'workspace' in root_manifest:
		print(f'❌ No workspace found in root {root_manifest_path}')
		sys.exit(1)

	if not 'members' in root_manifest['workspace']:
		return []
	
	members = []
	for member in root_manifest['workspace']['members']:
		if member in exclude:
			print(f'❌ Excluded member should not appear in the workspace {member}')
			sys.exit(1)
		members.append(member)
	
	return members

# List all members of the workspace.
# Return: Map name -> (path, manifest)
def get_crates(workspace_dir, exclude_crates) -> dict:
	crates = {}

	for root, dirs, files in os.walk(workspace_dir):
		if "target" in root:
			continue
		for file in files:
			if file != "Cargo.toml":
				continue

			path = os.path.join(root, file)
			with open(path, "r") as f:
				content = f.read()
				manifest = toml.loads(content)
			
			if 'workspace' in manifest:
				if root != workspace_dir:
					print("⏩ Excluded recursive workspace at %s" % path)
				continue
			
			# Cut off the root path and the trailing /Cargo.toml.
			path = path[len(workspace_dir)+1:-11]
			name = manifest['package']['name']
			if path in exclude_crates:
				print("⏩ Excluded crate %s at %s" % (name, path))
				continue
			crates[name] = (path, manifest)
	
	return crates

# Check that there are no duplicate entries in the workspace.
def check_duplicates(workspace_crates):
	print(f'🔎 Checking for duplicate crates')
	found = {}
	for path in workspace_crates:
		if path in found:
			print(f'❌ crate is listed twice in the workspace {path}')
			sys.exit(1)
		found[path] = True

# Check that all crates are in the workspace.
def check_missing(workspace_crates, all_crates):
	print(f'🔎 Checking for missing crates')
	if len(workspace_crates) == len(all_crates):
		print(f'✅ All {len(all_crates)} crates are in the workspace')
		return

	missing = []
	# Find out which ones are missing.
	for name, (path, manifest) in all_crates.items():
		if not path in workspace_crates:
			missing.append([name, path, manifest])
	missing.sort()

	for name, path, _manifest in missing:
		print("❌ %s in %s" % (name, path))
	print(f'😱 {len(all_crates) - len(workspace_crates)} crates are missing from the workspace')
	sys.exit(1)

# Check that all local dependencies are good.
def check_links(all_crates):
	print(f'🔎 Checking for broken dependency links')
	links = []
	broken = []

	for name, (path, manifest) in all_crates.items():
		def check_deps(deps):
			for dep in deps:
				# Could be renamed:
				dep_name = dep
				if 'package' in deps[dep]:
					dep_name = deps[dep]['package']
				if dep_name in all_crates:
					links.append((name, dep_name))

					if not 'path' in deps[dep]:
						broken.append((name, dep_name, "crate must be linked via `path`"))
						return
		
		def check_crate(deps):
			to_checks = ['dependencies', 'dev-dependencies', 'build-dependencies']

			for to_check in to_checks:
				if to_check in deps:
					check_deps(deps[to_check])
		
		# There could possibly target dependant deps:
		if 'target' in manifest:
			# Target dependant deps can only have one level of nesting:
			for _, target in manifest['target'].items():
				check_crate(target)
		
		check_crate(manifest)

		

	links.sort()
	broken.sort()

	if len(broken) > 0:
		for (l, r, reason) in broken:
			print(f'❌ {l} -> {r} ({reason})')

		print("💥 %d out of %d links are broken" % (len(broken), len(links)))
		sys.exit(1)
	else:
		print("✅ All %d internal dependency links are correct" % len(links))

if __name__ == "__main__":
	args = parse_args()
	main(args[0], args[1])
