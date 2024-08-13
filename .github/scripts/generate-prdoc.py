#!/usr/bin/env python3

"""
Generate the PrDoc for a Pull Request with a specific number, audience and bump level.

It downloads and parses the patch from the GitHub API to opulate the prdoc with all modified crates.
This will delete any prdoc that already exists for the PR if `--force` is passed.

Usage:
	python generate-prdoc.py --pr 1234 --audience "TODO" --bump "TODO"
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

	prdoc = { "doc": [{}], "crates": [] }

	prdoc["title"] = title
	prdoc["doc"][0]["audience"] = audience
	prdoc["doc"][0]["description"] = description

	workspace = Workspace.from_path(".")

	modified_paths = []
	for diff in whatthepatch.parse_patch(patch):
		modified_paths.append(diff.header.new_path)

	modified_crates = {}
	for p in modified_paths:
		# Go up until we find a Cargo.toml
		p = os.path.join(workspace.path, p)
		while not os.path.exists(os.path.join(p, "Cargo.toml")):
			p = os.path.dirname(p)
		
		with open(os.path.join(p, "Cargo.toml")) as f:
			manifest = toml.load(f)
		
		if not "package" in manifest:
			print(f"File was not in any crate: {p}")
			continue
		
		crate_name = manifest["package"]["name"]
		if workspace.crate_by_name(crate_name).publish:
			modified_crates[crate_name] = True
		else:
			print(f"Skipping unpublished crate: {crate_name}")

	print(f"Modified crates: {modified_crates.keys()}")

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
		yaml.dump(prdoc, f)

def parse_args():
	parser = argparse.ArgumentParser()
	parser.add_argument("--pr", type=int, required=True)
	parser.add_argument("--audience", type=str, default="TODO")
	parser.add_argument("--bump", type=str, default="TODO")
	parser.add_argument("--force", type=str)
	return parser.parse_args()

if __name__ == "__main__":
	args = parse_args()
	force = True if args.force.lower() == "true" else False
	print(f"Args: {args}, force: {force}")
	from_pr_number(args.pr, args.audience, args.bump, force)
