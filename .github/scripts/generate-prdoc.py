#!/usr/bin/env python3

"""
Generate the PrDoc for a Pull Request with a specific number, audience and bump level.

It downloads and parses the patch from the GitHub API to opulate the prdoc with all modified crates.
This will delete any prdoc that already exists for the PR if `--force` is passed.

Usage:
	python generate-prdoc.py --pr 1234 --audience node_dev --bump patch
	python generate-prdoc.py --pr 1234 --audience runtime_dev --bump minor --topic xcm
"""

import argparse
import os
import re
import sys
import toml
import yaml
import requests

from github import Github
import whatthepatch
from cargo_workspace import Workspace

# Sibling module; this script is also loaded by cmd/cmd.py from another directory.
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import prdoc_topics

# Validate `--topic` against the taxonomy and normalize it to the topic id. The taxonomy is read
# from the working directory on purpose: `/cmd` runs this script from the `cmd-bot` branch against
# a checkout of the PR branch, and the taxonomy must be the PR's.
def translate_topic(topic):
	if topic is None or not topic.strip():
		return None

	try:
		topics, match_topic = prdoc_topics.load_topic_matcher(".")
	except prdoc_topics.TaxonomyUnavailable as e:
		raise Exception(f"`--topic` is not available on this branch: {e}")

	matched = match_topic(topic, topics)
	if matched is None:
		ids = ", ".join(prdoc_topics.topic_ids(topics))
		raise Exception(f"Unknown topic '{topic}'. Valid topic ids (see {prdoc_topics.TOPICS_PATH}): {ids}")

	print(f"Translated topic '{topic}' to '{matched['id']}'")
	return matched["id"]

# Download the patch and pass the info into `create_prdoc`.
def from_pr_number(n, audience, bump, force, topic=None):
	# Before any network access, so a typo fails fast.
	topic = translate_topic(topic)

	print(f"Fetching PR '{n}' from GitHub")
	g = Github()

	repo = g.get_repo("paritytech/polkadot-sdk")
	pr = repo.get_pull(n)

	patch_url = pr.patch_url
	patch = requests.get(patch_url).text

	create_prdoc(n, audience, pr.title, pr.body, patch, bump, force, topic)

def translate_audience(audience):
	aliases = {
		'runtime_dev': 'Runtime Dev',
		'runtime_user': 'Runtime User',
		'node_dev': 'Node Dev',
		'node_operator': 'Node Operator',
		'todo': 'Todo',
	}

	mapped = [aliases.get(a) for a in audience]
	if len(mapped) == 1:
		mapped = mapped[0]

	print(f"Translated audience '{audience}' to '{mapped}'")
	return mapped

def create_prdoc(pr, audience, title, description, patch, bump, force, topic=None):
	path = f"prdoc/pr_{pr}.prdoc"

	if os.path.exists(path):
		if force == True:
			print(f"Overwriting existing PrDoc for PR {pr}")
		else:
			raise Exception(f"PrDoc already exists for PR {pr}. Use --force to overwrite.")
	else:
		print(f"No preexisting PrDoc for PR {pr}")

	prdoc = { "title": title }
	if topic is not None:
		prdoc["topic"] = topic
	prdoc["doc"] = [{}]
	prdoc["crates"] = []
	audience = translate_audience(audience)

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
				raise Exception("No Cargo.toml found for a modified path (walked up to /)")
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
	allowed_audiences = ["runtime_dev", "runtime_user", "node_dev", "node_operator", "todo"]
	if parser is None:
		parser = argparse.ArgumentParser()
	parser.add_argument("--pr", type=int, required=pr_required, help="The PR number to generate the PrDoc for.")
	parser.add_argument("--audience", type=str, nargs='*', choices=allowed_audiences, default=["todo"], help="The audience of whom the changes may concern. Example: --audience runtime_dev node_dev")
	parser.add_argument("--bump", type=str, default="major", choices=["patch", "minor", "major", "silent", "ignore", "none"], help="A default bump level for all crates. Example: --bump patch")
	parser.add_argument("--force", action="store_true", help="Whether to overwrite any existing PrDoc.")
	parser.add_argument("--topic", type=str, default=None, help="Optional changelog topic, one of the ids in prdoc/topics.yaml. Omit it to let the changelog derive the topic from the modified crates. Example: --topic xcm")
	return parser

def snake_to_title(s):
	return ' '.join(word.capitalize() for word in s.split('_'))

# `log` lets cmd.py forward the failure reason to the PR comment instead of a bare "failed".
def main(args, log=print):
	print(f"Args: {args}, force: {args.force}")
	setup_yaml()
	try:
		from_pr_number(args.pr, args.audience, args.bump, args.force, args.topic)
		return 0
	except Exception as e:
		log(f"Error generating prdoc: {e}")
		return 1

if __name__ == "__main__":
	parser = setup_parser()
	args = parser.parse_args()
	sys.exit(main(args))
