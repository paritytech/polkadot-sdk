# Polkadot Zombienet SDK Tests

This crate contains integration tests for Polkadot using the Zombienet SDK.

## Building Required Binaries (for `native` provider)

Run these commands from the Polkadot SDK workspace root:

```bash
# Build Polkadot Parachain node executable
cargo build --release --locked -p polkadot-parachain-bin --bin polkadot-parachain

# Build Undying collator
cargo build --release -p test-parachain-undying-collator

# Build Adder collator
cargo build --release -p test-parachain-adder-collator

# Build Malus executable
cargo build --release -p polkadot-test-malus
```
*Remember to ensure the resulting binaries (e.g., `./target/release/polkadot-parachain`) are accessible via your system's `PATH`.*

**Example:** You can add the build output directory to your PATH for the current terminal session by running the following command from the workspace root:

```bash
export PATH="$(pwd)/target/release:$PATH"
```

This allows Zombienet (`native` provider) to find the executables. You generally need to run this in the same terminal session where you intend to run the tests.

## Running the Tests

Navigate to the root of the Polkadot SDK workspace.

### Run all tests:

```bash
ZOMBIE_PROVIDER=native cargo test --release -p polkadot-zombienet-sdk-tests --features zombie-ci
```

### Run a specific test:

Replace `[test_module::test_name]` with the actual path to the test function you want to run (e.g., `elastic_scaling::basic_3cores_test`).

```bash
ZOMBIE_PROVIDER=native cargo test --release -p polkadot-zombienet-sdk-tests --features zombie-ci [test_module::test_name]
```
Example:
```bash
ZOMBIE_PROVIDER=native cargo test --release -p polkadot-zombienet-sdk-tests --features zombie-ci elastic_scaling::basic_3cores_test
```

### Running tests requiring specific environment variables:

Some tests might require specific environment variables. Check test-specific logic or error messages for requirements.

-   **Example:** The `functional::approved_peer_mixed_validators::approved_peer_mixed_validators_test` requires the `OLD_POLKADOT_IMAGE` environment variable.

```bash
# Example for a test needing OLD_POLKADOT_IMAGE
OLD_POLKADOT_IMAGE=docker.io/parity/polkadot:v1.7.0 ZOMBIE_PROVIDER=native cargo test --release -p polkadot-zombienet-sdk-tests --features zombie-ci functional::approved_peer_mixed_validators::approved_peer_mixed_validators_test
```

## Notes

-   `--release`: Builds the test harness and potentially the runtimes in release mode.
-   `-p polkadot-zombienet-sdk-tests`: Specifies the package to test.
-   `--features zombie-ci`: Enables features required for the tests, including metadata generation via the `build.rs` script.
-   `ZOMBIE_PROVIDER=native`: Tells Zombienet to use locally built binaries (which must be in your `PATH`). You might use `podman` or `docker` as alternatives if configured, which may avoid the need to build binaries manually.
