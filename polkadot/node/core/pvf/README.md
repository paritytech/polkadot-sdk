# PVF Host

This is the PVF host, responsible for responding to requests from Candidate
Validation and spawning worker tasks to fulfill those requests.

See also:

- for more information: [the Implementer's Guide][impl-guide]
- for an explanation of terminology: [the Glossary][glossary]

## Running basic tests

Running `cargo test` in the `pvf/` directory will run unit and integration
tests.

**Note:** some tests run only under Linux, amd64, and/or with the
`ci-only-tests` feature enabled.

See the general [Testing][testing] instructions for more information on
**running tests** and **observing logs**.

## Running a test-network with zombienet

Since this crate is consensus-critical, for major changes it is highly
recommended to run a test-network. See the "Behavior tests" section of the
[Testing][testing] docs for full instructions.

To run the PVF-specific zombienet test:

```sh
RUST_LOG=parachain::pvf=trace zombienet --provider=native spawn zombienet_tests/functional/0001-parachains-pvf.toml
```

## Testing on Linux

Some of the PVF functionality, especially related to security, is Linux-only,
and some is amd64-only. If you touch anything security-related, make sure to
test on Linux amd64! If you're on a Mac, you can either run a VM or you can hire
a VPS and use the open-source tool [EternalTerminal][et] to connect to it.[^et]

[^et]: Unlike ssh, ET preserves your session across disconnects, and unlike
another popular persistent shell, mosh, it allows scrollback.

[impl-guide]: https://paritytech.github.io/polkadot-sdk/book/pvf-prechecking.html#summary
[glossary]: https://paritytech.github.io/polkadot-sdk/book/glossary.html
[testing]: https://github.com/paritytech/polkadot-sdk/blob/master/polkadot/doc/testing.md
[et]: https://github.com/MisterTea/EternalTerminal
