# xcm-emulator

XCM-Emulator is a tool to emulate XCM program execution using
pre-configured runtimes, including those used to run on live
networks, such as Kusama, Polkadot, Asset Hubs, et cetera.
This allows for testing cross-chain message passing and verifying
outcomes, weights, and side-effects. It is faster than spinning up
a zombienet and as all the chains are in one process debugging using Clion is easy.

## Limitations

As the messages do not physically go through the same messaging infrastructure
there is some code that is not being tested compared to using slower E2E tests.
In future it may be possible to run these XCM emulated tests as E2E tests (without changes).

As well as the XCM message transport being mocked out, so too are areas around consensus,
in particular things like disputes, staking and iamonline events can't be tested.

## Alternatives

If you just wish to test execution of various XCM instructions
against the XCM VM then the `xcm-simulator` (in the Polkadot
repo) is the perfect tool for this.
