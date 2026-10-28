'''
Shared access to the changelog topic taxonomy (`prdoc/topics.yaml`) for the prdoc tooling.

The taxonomy and its matching rules are owned by the changelog generator
(`scripts/release/changelog/generate.py`). This module loads both from a checkout so that
`check-prdoc.py` and `generate-prdoc.py` accept exactly the `topic` values the changelog accepts.
'''

import importlib.util
import os

TOPICS_PATH = 'prdoc/topics.yaml'
CHANGELOG_GENERATOR = 'scripts/release/changelog/generate.py'

class TaxonomyUnavailable(Exception):
	'''
	The checkout has no usable taxonomy (a branch that predates it). Callers decide whether that
	is an error: the PR check skips, `--topic` refuses.
	'''

def load_topic_matcher(repo_root='.'):
	'''
	Returns `(topics, match_topic)` from the generator in `repo_root`; `match_topic(value, topics)`
	gives the matching topic or `None`. Raises `TaxonomyUnavailable` when the checkout predates the
	taxonomy or the matcher; a malformed taxonomy raises a plain `Exception`.
	'''
	taxonomy = os.path.join(repo_root, TOPICS_PATH)
	generator = os.path.join(repo_root, CHANGELOG_GENERATOR)
	for path in (taxonomy, generator):
		if not os.path.exists(path):
			raise TaxonomyUnavailable(f'{path} not found in this checkout')

	spec = importlib.util.spec_from_file_location('changelog_generate', generator)
	module = importlib.util.module_from_spec(spec)
	spec.loader.exec_module(module)
	match_topic = getattr(module, 'match_topic', None)
	if match_topic is None:
		raise TaxonomyUnavailable(f'{generator} predates topic matching (no `match_topic`)')

	try:
		topics = module.load_topics(taxonomy)
	except SystemExit as e:
		# The generator is a CLI and exits on a malformed taxonomy; report that as an error.
		raise Exception(f'{taxonomy} could not be loaded: {e}')

	return topics, match_topic

def topic_ids(topics):
	return [t['id'] for t in topics]
