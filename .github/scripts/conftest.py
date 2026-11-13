"""Isolate script imports and their optional network/workspace dependencies."""

import importlib.util
from pathlib import Path
import sys
from unittest.mock import MagicMock

import pytest


@pytest.fixture
def load_script(monkeypatch):
	scripts = Path(__file__).parent
	monkeypatch.syspath_prepend(str(scripts))
	for name in ("toml", "requests", "github", "whatthepatch", "cargo_workspace"):
		monkeypatch.setitem(sys.modules, name, MagicMock())

	def load(relative):
		path = scripts / relative
		monkeypatch.syspath_prepend(str(path.parent))
		# Avoid shadowing the standard-library cmd module used by pytest/pdb.
		spec = importlib.util.spec_from_file_location("ci_" + path.stem.replace("-", "_"), path)
		module = importlib.util.module_from_spec(spec)
		spec.loader.exec_module(module)
		return module

	return load
