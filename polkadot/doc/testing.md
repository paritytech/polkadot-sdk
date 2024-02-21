# Testing Polkadot

Testing is a crucial part of ensuring the correctness and robustness of the Polkadot codebase. This document outlines our comprehensive testing strategy, covering different scopes and techniques.

## Testing Scopes
### 1. Unit testing
- Small-scale tests ensuring the correctness of individual functions.
- Usually involves running `cargo test` in the crate you are testing.
- For full coverage you may have to pass some additional features. For example: 
```sh 
cargo test --features ci-only-tests
```

### 2. Integration tests
- Tests interactions between components within the Polkadot system.  We have two primary integration testing methods:
  - **Subsystem Testing**:
    - Focuses on a single subsystem while mocking the interactions with other parts of the system. This helps validate incoming and outgoing messages for the subsystem under test.
  - **Behavior Testing**:
    - Involves launching small-scale networks with multiple nodes, some of which may be adversarial.
    - Useful for testing system behavior under various conditions, including error handling and resilience to unexpected situations.


## Running Behavior Tests
Currently, we often use **zombienet** to run behavior tests both locally and in CI. Here's how:
1. Make sure you have [Zombienet][zombienet] installed
2. Build Required Binaries: Build all necessary binaries from the `polkadot/` project directory:
```sh
cargo install --path . --locked
```
3. Install Additional Dependencies (if needed): Install specific binaries required for your tests. For example, to install `undying-collator` run the following from `polkadot/`:
```sh
cargo install --path ./parachain/test-parachains/undying/collator --locked
```
4. Execute the test: Run the zombienet test from the `polkadot` directory:
```sh
RUST_LOG=parachain::pvf=trace zombienet --provider=native spawn zombienet_tests/functional/0001-parachains-pvf.toml
```
5. **Monitoring**: Track validator logs or metrics to ensure expected behavior 
```sh
tail -f <log_file>
```

## Observing Logs

For detailed test analysis, review the logs:

1. **Add Logging**: Initialize logging at the start of the test:
```rust
sp_tracing::try_init_simple();
```
2. **Run Test with Logging**: Specify the target and level of logging:
```sh
RUST_LOG=parachain::pvf=trace cargo test execute_can_run_serially 
```
For more info on how our logs work, check [the docs][logs].

## Code Coverage

Code coverage helps identify areas that are well-covered by tests. While there are limitations, we use the following tools:
- **Tarpaulin**: State-of-the-art tool, but may provide false negatives.
- **Rust's [MIR based coverage tooling](
  https://blog.rust-lang.org/inside-rust/2020/11/12/source-based-code-coverage.html)**: A newer approach that offers improved accuracy.

### Generating a code coverage report:
```sh
# setup
rustup component add llvm-tools-preview
cargo install grcov miniserve

export CARGO_INCREMENTAL=0
# wasm is not happy with the instrumentation
export SKIP_BUILD_WASM=true
export BUILD_DUMMY_WASM_BINARY=true
# the actully collected coverage data
export LLVM_PROFILE_FILE="llvmcoveragedata-%p-%m.profraw"
# build wasm without instrumentation
export WASM_TARGET_DIRECTORY=/tmp/wasm
cargo +nightly build
# required rust flags
export RUSTFLAGS="-Zinstrument-coverage"
# assure target dir is clean
rm -r target/{debug,tests}
# run tests to get coverage data
cargo +nightly test --all

# create the *html* report out of all the test binaries
# mostly useful for local inspection
grcov . --binary-path ./target/debug -s . -t html --branch --ignore-not-existing -o ./coverage/
miniserve -r ./coverage

# create a *codecov* compatible report
grcov . --binary-path ./target/debug/ -s . -t lcov --branch --ignore-not-existing --ignore "/*" -o lcov.info
```

The test coverage in `lcov` can the be published to <https://codecov.io>.

```sh
bash <(curl -s https://codecov.io/bash) -f lcov.info
```

or just printed as part of the PR using a github action i.e.
[`jest-lcov-reporter`](https://github.com/marketplace/actions/jest-lcov-reporter).

For full examples on how to use [`grcov` /w Polkadot specifics see the github
repo](https://github.com/mozilla/grcov#coverallscodecov-output).

## Fuzzing

Fuzzing is a testing technique that uses random or semi-random inputs to identify potential vulnerabilities in code.

### Current targets:
- `erasure-coding`
### Tooling
We use `honggfuzz-rs` for its speed,  which is important when fuzzing is included in our development process.
### Limitations
Fuzzing is generally less effective for code sections that rely heavily on cryptographic hashes or signatures. This is because the vast majority of random inputs will be invalid, requiring careful crafting of input data for meaningful testing. System-level fuzzing can also be challenging due to the complexity of managing state.

## Performance metrics

We take performance seriously and use several tools to track and optimize it:
- `criterion` For precise timing measurements of code execution.
- **`iai` harness or `criterion-perf`** To analyze cache behavior (hits/misses) which can significantly impact performance.
- `coz` A specialized compiler focused on performance analysis.

While `coz` shows promise for runtime analysis, the sheer complexity of our system makes it challenging to get meaningful insights in a reasonable timeframe. However, we're exploring an innovative approach:
- Replay-Based Testing: This involves recording network traffic and replaying it at high speed to gather extensive performance data. This technique could help us pinpoint areas for optimization without the need for complex mocking setups.

**The Future of Performance Testing**: We're committed to continuously improving our performance testing methods. While our current approach has limitations, we're always investigating new ways to make our code faster and more efficient.

## Writing Small-Scope Integration Tests

### Key Requirements:
- **Customizable Node Behavior**: We need the ability to easily create test nodes that exhibit specific behaviors.
- **Flexible Configuration**: Support for different configuration options to tailor test scenarios.
- **Extensibility**: The system should be designed for easy expansion through external crates.

### Implementation approaches
1. **MVP (Minimum Viable Product):**
- A straightforward builder pattern can be used to customize test nodes and replace subsystems as needed.
2. **Full `proc-macro` implementation:**
- **Goal**: A powerful system that streamlines test node creation and configuration.
- **Approach**: Following the common `Overseer` pattern, use a `proc-macro` to generate the `AllSubsystems` type and implicitly create the `AllMessages` enum.
- **Status:** Currently in development, see the [implementation PR](https://github.com/paritytech/polkadot/pull/2962)
for details.

#### Overseer implementation example:

```rust
struct BehaveMaleficient;

impl OverseerGen for BehaveMaleficient {
 fn generate<'a, Spawner, RuntimeClient>(
  &self,
  args: OverseerGenArgs<'a, Spawner, RuntimeClient>,
 ) -> Result<(Overseer<Spawner, Arc<RuntimeClient>>, OverseerHandler), Error>
 where
  RuntimeClient: 'static + ProvideRuntimeApi<Block> + HeaderBackend<Block> + AuxStore,
  RuntimeClient::Api: ParachainHost<Block> + BabeApi<Block> + AuthorityDiscoveryApi<Block>,
  Spawner: 'static + overseer::gen::Spawner + Clone + Unpin,
 {
  let spawner = args.spawner.clone();
  let leaves = args.leaves.clone();
  let runtime_client = args.runtime_client.clone();
  let registry = args.registry.clone();
  let candidate_validation_config = args.candidate_validation_config.clone();
  // modify the subsystem(s) as needed:
  let all_subsystems = create_default_subsystems(args)?.
        // or spawn an entirely new set

        replace_candidate_validation(
   // create the filtered subsystem
   FilteredSubsystem::new(
    CandidateValidationSubsystem::with_config(
     candidate_validation_config,
     Metrics::register(registry)?,
    ),
                // an implementation of
    Skippy::default(),
   ),
  );

  Overseer::new(leaves, all_subsystems, registry, runtime_client, spawner)
   .map_err(|e| e.into())

        // A builder pattern will simplify this further
        // WIP https://github.com/paritytech/polkadot/pull/2962
 }
}

fn main() -> eyre::Result<()> {
 color_eyre::install()?;
 let cli = Cli::from_args();
 assert_matches::assert_matches!(cli.subcommand, None);
 polkadot_cli::run_node(cli, BehaveMaleficient)?;
 Ok(())
}
```

A fully working example can be found in [`variant-a`](../node/malus/src/variant-a.rs).


[zombienet]: https://github.com/paritytech/zombienet
[logs]: https://github.com/paritytech/polkadot-sdk/blob/master/polkadot/node/gum/src/lib.rs
