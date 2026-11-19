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
import sys
import yaml
import argparse
import cargo_workspace

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import prdoc_topics

def check_prdoc_topics(repo_root, paths):
	'''
	Warn about `topic` values that match nothing in `prdoc/topics.yaml`. This never fails: the
	field is free-form by design and the changelog falls back to deriving the topic from the
	crates, so a typo costs a warning annotation on the PR, not a red CI.
	'''
	try:
		topics, match_topic = prdoc_topics.load_topic_matcher(repo_root)
	except prdoc_topics.TaxonomyUnavailable as e:
		print(f'ℹ️ Skipping the topic check: {e}')
		return
	except Exception as e:
		# Release tooling must never break the PR check, but a broken check has to stay visible.
		print(f'::warning::Skipping the topic check, the taxonomy could not be loaded: {e}')
		return

	known = ', '.join(prdoc_topics.topic_ids(topics))
	for path in paths:
		with open(path, 'r') as f:
			prdoc = yaml.safe_load(f)
		if not isinstance(prdoc, dict):
			# Not this check's business; the schema and crate checks report it.
			continue

		topic = prdoc.get('topic')
		if topic in (None, ''):
			continue
		if match_topic(topic, topics) is None:
			print(f'::warning file={path}::Unknown topic {topic!r}, the changelog will derive the topic from the crates instead. Known topic ids: {known} (see {prdoc_topics.TOPICS_PATH}).')
		else:
			print(f'🏷️ {path} has topic {topic!r}.')

def check_prdoc_crate_names(root, paths):
	'''
	Check that all crates of the `crates` section of each prdoc is present in the workspace.
	'''
	
	print(f'🔎 Reading workspace {root}.')
	workspace = cargo_workspace.Workspace.from_path(root)
	crate_names = [crate.name for crate in workspace.crates]

	print(f'📦 Checking {len(paths)} prdocs against {len(crate_names)} crates.')
	faulty = {}

	for path in paths:
		with open(path, 'r') as f:
			prdoc = yaml.safe_load(f)

		for crate in prdoc.get('crates', []):
			crate = crate['name']
			if crate in crate_names:
				continue

			faulty.setdefault(path, []).append(crate)

	if len(faulty) == 0:
		print('✅ All prdocs are valid.')
	else:
		print('❌ Some prdocs are invalid.')
		for path, crates in faulty.items():
			print(f'💥 {path} lists invalid crate: {", ".join(crates)}')
		exit(1)

def parse_args():
	parser = argparse.ArgumentParser(description='Check prdoc files')
	parser.add_argument('root', help='The cargo workspace manifest', metavar='root', type=str, nargs=1)
	parser.add_argument('prdoc', help='The prdoc files', metavar='prdoc', type=str, nargs='*')
	args = parser.parse_args()

	if len(args.prdoc) == 0:
		print('❌ Need at least one prdoc file as argument.')
		exit(1)

	return { 'root': os.path.abspath(args.root[0]), 'prdocs': args.prdoc }

if __name__ == '__main__':
	args = parse_args()
	root = args['root']
	repo_root = root if os.path.isdir(root) else os.path.dirname(root)
	check_prdoc_topics(repo_root, args['prdocs'])
	check_prdoc_crate_names(root, args['prdocs'])
