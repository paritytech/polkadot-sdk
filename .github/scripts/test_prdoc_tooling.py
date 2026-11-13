"""Trusted-code loading and failure-message behavior shared by the prdoc tooling."""

import argparse
from pathlib import Path
import textwrap

import pytest

import prdoc_topics


@pytest.fixture
def write(tmp_path):
	def create(relative, content):
		path = tmp_path / relative
		path.parent.mkdir(parents=True, exist_ok=True)
		path.write_text(textwrap.dedent(content))
		return path
	return create


def test_taxonomy_from_checkout_but_generator_from_own_tree(tmp_path, write):
	write(prdoc_topics.TOPICS_PATH, """
		topics:
		  - id: only-here
		    label: Only In This Checkout
		    aliases: [oh]
		  - id: misc
		    label: Misc
	""")
	marker = tmp_path / "EXECUTED"
	# Executing the PR checkout's generator would leave a marker and taint the result.
	write(prdoc_topics.CHANGELOG_GENERATOR, f"""
		import pathlib
		pathlib.Path({str(marker)!r}).write_text('executed')
		def match_topic(value, topics):
			return topics[0]
		def load_topics(path):
			return [{{'id': 'from-hostile-code', 'label': 'x', 'aliases': []}}]
	""")
	topics, match_topic = prdoc_topics.load_topic_matcher(str(tmp_path))
	assert not marker.exists(), "The generator from the checkout under review was executed"
	assert prdoc_topics.topic_ids(topics) == ["only-here", "misc"]
	assert match_topic("OH", topics)["id"] == "only-here"
	assert match_topic("from-hostile-code", topics) is None


def test_generator_is_the_one_next_to_the_tooling():
	expected = Path(__file__).resolve().parents[2]
	assert Path(prdoc_topics.OWN_REPO_ROOT) == expected
	assert (expected / prdoc_topics.CHANGELOG_GENERATOR).exists()


@pytest.mark.parametrize("malformed", [False, True], ids=["missing", "malformed"])
def test_unavailable_or_malformed_taxonomy(tmp_path, write, malformed):
	if malformed:
		write(prdoc_topics.TOPICS_PATH, "topics: []\n")
	with pytest.raises(Exception) as error:
		prdoc_topics.load_topic_matcher(str(tmp_path))
	assert isinstance(error.value, prdoc_topics.TaxonomyUnavailable) is not malformed


@pytest.mark.parametrize("reason,topic,expected", [
	("Unknown topic 'x'. Valid topic ids: a,\nEOF\nsubweight=injected\n" + "b" * 5000,
		"x", "Error generating prdoc: Unknown topic 'x'."),
	("boom", None, "Error generating prdoc: boom"),
], ids=["hostile", "harmless"])
def test_forwarded_failure_reason(load_script, monkeypatch, reason, topic, expected):
	generate = load_script("generate-prdoc.py")
	def fail(*args):
		raise Exception(reason)
	monkeypatch.setattr(generate, "from_pr_number", fail)
	args = argparse.Namespace(pr=1, audience=["todo"], bump="patch", force=False, topic=topic)
	logged = []
	assert generate.main(args, log=logged.append) == 1
	assert len(logged) == 1
	assert logged[0].startswith(expected)
	assert "\n" not in logged[0] and "\r" not in logged[0]
	assert len(logged[0]) <= 1003
	if topic is None:
		assert logged == [expected]
