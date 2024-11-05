#!/usr/bin/env python3

"""
Generate the PrDoc for a Pull Request with a specific number, audience and bump level.

It downloads and parses the patch from the GitHub API to opulate the prdoc with all modified crates.
This will delete any prdoc that already exists for the PR if `--force` is passed.

Usage:
	python generate-prdoc.py --pr 1234 --audience node_dev --bump patch
"""

import argparse
import os
import re
import sys
import subprocess
import toml
import yaml
import requests

from github import Github
import whatthepatch
from cargo_workspace import Workspace

# Download the patch and pass the info into `create_prdoc`.
def from_pr_number(n, audience, bump, force):
	print(f"Fetching PR '{n}' from GitHub")
	g = Github()
	
	repo = g.get_repo("paritytech/polkadot-sdk")
	pr = repo.get_pull(n)

	patch_url = pr.patch_url
	patch = requests.get(patch_url).text

	create_prdoc(n, audience, pr.title, pr.body, patch, bump, force)

def create_prdoc(pr, audience, title, description, patch, bump, force):
	path = f"prdoc/pr_{pr}.prdoc"

	if os.path.exists(path):
		if force == True:
			print(f"Overwriting existing PrDoc for PR {pr}")
		else:
			print(f"PrDoc already exists for PR {pr}. Use --force to overwrite.")
			sys.exit(1)
	else:
		print(f"No preexisting PrDoc for PR {pr}")

	prdoc = { "title": title, "doc": [{}], "crates": [] }

	prdoc["doc"][0]["audience"] = audience
	prdoc["doc"][0]["description"] = description

	workspace = Workspace.from_path(".")

	modified_paths = []
	for diff in whatthepatch.parse_patch(patch):
		new_path = diff.header.new_path
		# Sometimes this lib returns `/dev/null` as the new path...
		if not new_path.startswith("/dev"):
			modified_paths.append(new_path)

	modified_crates = {}
	for p in modified_paths:
		# Go up until we find a Cargo.toml
		p = os.path.join(workspace.path, p)
		while not os.path.exists(os.path.join(p, "Cargo.toml")):
			if p == '/':
				exit(1)
			p = os.path.dirname(p)
		
		with open(os.path.join(p, "Cargo.toml")) as f:
			manifest = toml.load(f)
		
		if not "package" in manifest:
			continue
		
		crate_name = manifest["package"]["name"]
		if workspace.crate_by_name(crate_name).publish:
			modified_crates[crate_name] = True
		else:
			print(f"Skipping unpublished crate: {crate_name}")

	for crate_name in modified_crates.keys():
		entry = { "name": crate_name }

		if bump == 'silent' or bump == 'ignore' or bump == 'no change':
			entry["validate"] = False
		else:
			entry["bump"] = bump
		
		print(f"Adding crate {entry}")
		prdoc["crates"].append(entry)

	# write the parsed PR documentation back to the file
	with open(path, "w") as f:
		yaml.dump(prdoc, f, sort_keys=False)
		print(f"PrDoc for PR {pr} written to {path}")

# Make the `description` a multiline string instead of escaping \r\n.
def setup_yaml():
	def yaml_multiline_string_presenter(dumper, data):
		if len(data.splitlines()) > 1:
			data = '\n'.join([line.rstrip() for line in data.strip().splitlines()])
			return dumper.represent_scalar('tag:yaml.org,2002:str', data, style='|')
		return dumper.represent_scalar('tag:yaml.org,2002:str', data)

	yaml.add_representer(str, yaml_multiline_string_presenter)

# parse_args is also used by cmd/cmd.py
# if pr_required is False, then --pr is optional, as it can be derived from the PR comment body
def setup_parser(parser=None, pr_required=True):
	allowed_audiences = ["runtime_dev", "runtime_user", "node_dev", "node_operator"]
	if parser is None:
		parser = argparse.ArgumentParser()
	parser.add_argument("--pr", type=int, required=pr_required, help="The PR number to generate the PrDoc for.")
	parser.add_argument("--audience", type=str, nargs='*', choices=allowed_audiences, default=["todo"], help="The audience of whom the changes may concern. Example: --audience runtime_dev node_dev")
	parser.add_argument("--bump", type=str, default="major", choices=["patch", "minor", "major", "silent", "ignore", "no_change"], help="A default bump level for all crates. Example: --bump patch")
	parser.add_argument("--force", action="store_true", help="Whether to overwrite any existing PrDoc.")
	return parser

def snake_to_title(s):
	return ' '.join(word.capitalize() for word in s.split('_'))

def main(args):
	print(f"Args: {args}, force: {args.force}")
	setup_yaml()
	try:
		# Convert snake_case audience arguments to title case
		mapped_audiences = [snake_to_title(a) for a in args.audience]
		if len(mapped_audiences) == 1:
			mapped_audiences = mapped_audiences[0]
		from_pr_number(args.pr, mapped_audiences, args.bump, args.force)
		return 0
	except Exception as e:
		print(f"Error generating prdoc: {e}")
		return 1

if __name__ == "__main__":
	parser = setup_parser()
	args = parser.parse_args()
	main(args)
