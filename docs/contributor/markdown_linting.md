# Markdown linting

Since the introduction of [PR #1309](https://github.com/paritytech/polkadot-sdk/pull/1309), the markdown
files in this repository are checked by a linter for formatting and consistency.

The linter used is [`markdownlint`](https://github.com/DavidAnson/markdownlint) and can be installed locally on your
machine. It can also be setup as [pre-commit hook](https://github.com/igorshubovych/markdownlint-cli#use-with-pre-commit)
to ensure that your markdown is passing all the tests.

The rules in place are defined
[here](https://github.com/paritytech/polkadot-sdk/blob/master/.github/.markdownlint.yaml).

You may run `markdownlint` locally using:
```
markdownlint --config .github/.markdownlint.yaml --ignore target .
```

There are also plugins for your favorite editor, that can ensure that most
of the rules will pass and fix typical issues (such as trailing spaces,
missing eof new line, long lines, etc...)
