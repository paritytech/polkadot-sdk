# Python script tests

Use Python 3.11 or newer. From the repository root, install the pinned test dependencies in a
virtual environment and run the same command as CI:

```sh
ci_scripts_venv="$(mktemp -d)"
python3 -m venv "$ci_scripts_venv"
"$ci_scripts_venv/bin/python" -m pip install -r .github/scripts/requirements-test.txt
"$ci_scripts_venv/bin/python" -m pytest .github/scripts
```

To measure line and branch coverage without creating files in the checkout:

```sh
COVERAGE_FILE=/tmp/ci-scripts.coverage "$ci_scripts_venv/bin/python" -m coverage run --branch \
  --source=.github/scripts,scripts/release/changelog -m pytest .github/scripts
COVERAGE_FILE=/tmp/ci-scripts.coverage "$ci_scripts_venv/bin/python" -m coverage report
```

Tests use isolated imports, temporary directories and mocks for network/workspace dependencies;
they do not build runtimes or contact GitHub. The shared `load_script` fixture gives each test a
fresh script module without shadowing Python's standard-library `cmd` module. Fixtures restore
mocked modules and environment variables after each test.

`requirements-test.txt` pins the test runner, PyYAML, coverage and their transitive dependencies.
Runtime dependencies for bot commands remain in `generate-prdoc.requirements.txt`.
