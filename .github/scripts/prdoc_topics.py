'''
Shared access to the changelog topic taxonomy (`prdoc/topics.yaml`) for the prdoc tooling.

The taxonomy and its matching rules are owned by the changelog generator
(`scripts/release/changelog/generate.py`). This module loads the matching rules from the tree this
file is part of and the taxonomy from the checkout under review, so that `check-prdoc.py` and
`generate-prdoc.py` accept exactly the `topic` values the changelog accepts.

Keeping code and data apart matters for `/cmd prdoc`: the bot runs the scripts from a clone of the
trusted `cmd-bot` branch with a checkout of the PR branch as working directory. Anyone can trigger
`/cmd`, so the PR checkout is untrusted; only its `prdoc/topics.yaml` is read, nothing in it is
executed.
'''

import importlib.util
import os

TOPICS_PATH = 'prdoc/topics.yaml'
CHANGELOG_GENERATOR = 'scripts/release/changelog/generate.py'

# Root of the tree this module is part of (`.github/scripts/` -> repository root). In `/cmd` that is
# the `cmd-bot` clone, in the PR check it is the PR checkout itself.
OWN_REPO_ROOT = os.path.normpath(
	os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', '..'))

class TaxonomyUnavailable(Exception):
	'''
	No usable taxonomy or matcher (a branch that predates them). Callers decide whether that is an
	error: the PR check skips, `--topic` refuses.
	'''

def load_topic_matcher(repo_root='.'):
	'''
	Returns `(topics, match_topic)`: the topics of the taxonomy in `repo_root` and the matcher of
	this tree's generator; `match_topic(value, topics)` gives the matching topic or `None`. Raises
	`TaxonomyUnavailable` when either tree predates the taxonomy or the matcher; a malformed
	taxonomy raises a plain `Exception`.
	'''
	taxonomy = os.path.join(repo_root, TOPICS_PATH)
	if not os.path.exists(taxonomy):
		raise TaxonomyUnavailable(f'{taxonomy} not found in this checkout')

	# Deliberately not under `repo_root`: executing the generator of the checkout under review would
	# run untrusted code in `/cmd`. The matcher is stable; a PR changing it is validated by this copy.
	generator = os.path.join(OWN_REPO_ROOT, CHANGELOG_GENERATOR)
	if not os.path.exists(generator):
		raise TaxonomyUnavailable(f'{generator} not found next to the prdoc tooling')

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
