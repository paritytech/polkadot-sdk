'''
Tests for the prdoc tooling shared by `/cmd prdoc`, `check-prdoc.py` and `generate-prdoc.py`.

Run with: python3 .github/scripts/test_prdoc_tooling.py (needs PyYAML, nothing else).
'''

import argparse
import importlib.util
import os
import pathlib
import sys
import tempfile
import textwrap
import unittest
from unittest.mock import MagicMock, patch

SCRIPTS_DIR = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, SCRIPTS_DIR)
import prdoc_topics


def load_generate_prdoc():
	# The generator's network/workspace dependencies are not needed for these tests, nor are they
	# installed where the tests run (plain runner, like test_cmd.py).
	for name in ('toml', 'requests', 'github', 'whatthepatch', 'cargo_workspace'):
		sys.modules.setdefault(name, MagicMock())
	spec = importlib.util.spec_from_file_location(
		'generate_prdoc', os.path.join(SCRIPTS_DIR, 'generate-prdoc.py'))
	module = importlib.util.module_from_spec(spec)
	spec.loader.exec_module(module)
	return module


class TestLoadTopicMatcher(unittest.TestCase):
	'''
	`/cmd prdoc` runs the scripts from a trusted clone with an untrusted PR checkout as working
	directory: the taxonomy must come from the checkout, the code must not.
	'''

	def setUp(self):
		tmp = tempfile.TemporaryDirectory()
		self.addCleanup(tmp.cleanup)
		self.checkout = pathlib.Path(tmp.name)

	def write(self, rel, content):
		path = self.checkout / rel
		path.parent.mkdir(parents=True, exist_ok=True)
		path.write_text(textwrap.dedent(content))
		return path

	def test_taxonomy_from_checkout_but_generator_from_own_tree(self):
		self.write(prdoc_topics.TOPICS_PATH, '''
			topics:
			  - id: only-here
			    label: Only In This Checkout
			    aliases: [oh]
			  - id: misc
			    label: Misc
		''')
		marker = self.checkout / 'EXECUTED'
		# A hostile generator in the checkout: would taint the result and leave a marker if run.
		self.write(prdoc_topics.CHANGELOG_GENERATOR, f'''
			import pathlib
			pathlib.Path({str(marker)!r}).write_text('executed')
			def match_topic(value, topics):
				return topics[0]
			def load_topics(path):
				return [{{'id': 'from-hostile-code', 'label': 'x', 'aliases': []}}]
		''')

		topics, match_topic = prdoc_topics.load_topic_matcher(str(self.checkout))

		self.assertFalse(marker.exists(), 'the generator of the checkout under review was executed')
		self.assertEqual(prdoc_topics.topic_ids(topics), ['only-here', 'misc'])
		self.assertEqual(match_topic('OH', topics)['id'], 'only-here')
		self.assertIsNone(match_topic('from-hostile-code', topics))

	def test_generator_is_the_one_next_to_the_tooling(self):
		expected_root = os.path.normpath(os.path.join(SCRIPTS_DIR, '..', '..'))
		self.assertEqual(prdoc_topics.OWN_REPO_ROOT, expected_root)
		self.assertTrue(os.path.exists(
			os.path.join(expected_root, prdoc_topics.CHANGELOG_GENERATOR)))

	def test_missing_taxonomy_is_unavailable_not_an_error(self):
		with self.assertRaises(prdoc_topics.TaxonomyUnavailable):
			prdoc_topics.load_topic_matcher(str(self.checkout))

	def test_malformed_taxonomy_is_an_error(self):
		self.write(prdoc_topics.TOPICS_PATH, 'topics: []\n')
		with self.assertRaises(Exception) as cm:
			prdoc_topics.load_topic_matcher(str(self.checkout))
		self.assertNotIsInstance(cm.exception, prdoc_topics.TaxonomyUnavailable)


class TestForwardedFailureReason(unittest.TestCase):
	'''
	cmd.py forwards the failure reason into the bot's PR comment through a heredoc in
	$GITHUB_OUTPUT; the reason may quote the untrusted checkout's taxonomy.
	'''

	def test_log_message_is_one_bounded_line(self):
		generate_prdoc = load_generate_prdoc()
		hostile = "Unknown topic 'x'. Valid topic ids: a,\nEOF\nsubweight=injected\n" + 'b' * 5000
		args = argparse.Namespace(pr=1, audience=['todo'], bump='patch', force=False, topic='x')
		logged = []

		with patch.object(generate_prdoc, 'from_pr_number', side_effect=Exception(hostile)):
			self.assertEqual(generate_prdoc.main(args, log=logged.append), 1)

		self.assertEqual(len(logged), 1)
		self.assertNotIn('\n', logged[0])
		self.assertNotIn('\r', logged[0])
		self.assertTrue(logged[0].startswith("Error generating prdoc: Unknown topic 'x'."))
		self.assertLessEqual(len(logged[0]), 1003)

	def test_log_message_is_passed_through_when_harmless(self):
		generate_prdoc = load_generate_prdoc()
		args = argparse.Namespace(pr=1, audience=['todo'], bump='patch', force=False, topic=None)
		logged = []

		with patch.object(generate_prdoc, 'from_pr_number', side_effect=Exception('boom')):
			generate_prdoc.main(args, log=logged.append)

		self.assertEqual(logged, ['Error generating prdoc: boom'])


if __name__ == '__main__':
	unittest.main()
