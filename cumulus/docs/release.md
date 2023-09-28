# Releases

## Versioning

### Example #1

```
| Polkadot   | v  0. 9.22    |
| Client     | v  0. 9.22 0  |
| Runtime    | v     9 22 0  |  =>  9220
| semver     |    0. 9.22 0  |
```

### Example #2

```
| Polkadot   | v  0.10.42    |
| Client     | v  0.10.42 0  |
| Runtime    | v    10.42 0  |  => 10420
| semver     |    0.10.42 0  |
```

### Example #3

```
| Polkadot   | v  1. 2.18    |
| Client     | v  1. 2.18 0  |
| Runtime    | v  1  2 18 0  |  => 102180
| semver     |    1. 2.18 0  |
```


This document contains information related to the releasing process and describes a few of the steps and checks that are
performed during the release process.

## Client

### <a name="burnin"></a>Burn In

Ensure that Parity DevOps has run the new release on Westend and Kusama Asset Hub collators for 12h prior to publishing
the release.

### Build Artifacts

Add any necessary assets to the release. They should include:

- Linux binaries
    - GPG signature
    - SHA256 checksum
- WASM binaries of the runtimes
- Source code


## Runtimes

### Spec Version

A new runtime release must bump the `spec_version`. This may follow a pattern with the client release (e.g. runtime
v9220 corresponds to v0.9.22).

### Runtime version bump between RCs

The clients need to be aware of runtime changes. However, we do not want to bump the `spec_version` for every single
release candidate. Instead, we can bump the `impl` field of the version to signal the change to the client. This applies
only to runtimes that have been deployed.

### Old Migrations Removed

Previous `on_runtime_upgrade` functions from old upgrades should be removed.

### New Migrations

Ensure that any migrations that are required due to storage or logic changes are included in the `on_runtime_upgrade`
function of the appropriate pallets.

### Extrinsic Ordering & Storage

Offline signing libraries depend on a consistent ordering of call indices and functions. Compare the metadata of the
current and new runtimes and ensure that the `module index, call index` tuples map to the same set of functions. It also
checks if there have been any changes in `storage`. In case of a breaking change, increase `transaction_version`.

To verify the order has not changed, manually start the following
[Github Action](https://github.com/paritytech/polkadot-sdk/cumulus/.github/workflows/release-20_extrinsic-ordering-check-from-bin.yml).
It takes around a minute to run and will produce the report as artifact you need to manually check.

To run it, in the _Run Workflow_ dropdown:
1. **Use workflow from**: to ignore, leave `master` as default
2. **The WebSocket url of the reference node**: - Asset Hub Polkadot: `wss://statemint-rpc.polkadot.io`
    - Asset Hub Kusama: `wss://statemine-rpc.polkadot.io`
    - Asset Hub Westend: `wss://westmint-rpc.polkadot.io`
3. **A url to a Linux binary for the node containing the runtime to test**: Paste the URL of the latest
   release-candidate binary from the draft-release on Github. The binary has to previously be uploaded to S3 (Github url
   link to the binary is constantly changing)
    - E.g: https://releases.parity.io/cumulus/v0.9.270-rc3/polkadot-parachain
4. **The name of the chain under test. Usually, you would pass a local chain**: - Asset Hub Polkadot:
	`asset-hub-polkadot-local`
    - Asset Hub Kusama: `asset-hub-kusama-local`
    - Asset Hub Westend: `asset-hub-westend-local`
5. Click **Run workflow**

When the workflow is done, click on it and download the zip artifact, inside you'll find an `output.txt` file. The
things to look for in the output are lines like:

- `[Identity] idx 28 -> 25 (calls 15)` - indicates the index for Identity has changed
- `[+] Society, Recovery` - indicates the new version includes 2 additional modules/pallets.
- If no indices have changed, every modules line should look something like `[Identity] idx 25 (calls 15)`

**Note**: Adding new functions to the runtime does not constitute a breaking change as long as the indexes did not
change.

**Note**: Extrinsic function signatures changes (adding/removing & ordering arguments) are not caught by the job, so
those changes should be reviewed "manually"

### Benchmarks

The Benchmarks can now be started from the CI. First find the CI pipeline from
[here](https://gitlab.parity.io/parity/mirrors/cumulus/-/pipelines?page=1&scope=all&ref=release-parachains-v9220) and
pick the latest. [Guide](https://github.com/paritytech/ci_cd/wiki/Benchmarks:-cumulus)

### Integration Tests

Until https://github.com/paritytech/ci_cd/issues/499 is done, tests will have to be run manually.
1. Go to https://github.com/paritytech/parachains-integration-tests and check out the release branch. E.g.
https://github.com/paritytech/parachains-integration-tests/tree/release-v9270-v0.9.27 for `release-parachains-v0.9.270`
2. Clone `release-parachains-<version>` branch from Cumulus
3. `cargo build --release`
4. Copy `./target/polkadot-parachain` to `./bin`
5. Clone `it/release-<version>-fast-sudo` from Polkadot In case the branch does not exists (it is a manual process):
	cherry pick `paritytech/polkadot@791c8b8` and run:
	`find . -type f -name "*.toml" -print0 | xargs -0 sed -i '' -e 's/polkadot-vX.X.X/polkadot-v<version>/g'`
6. `cargo build --release --features fast-runtime`
7. Copy `./target/polkadot` into `./bin` (in Cumulus)
8. Run the tests:
   - Asset Hub Polkadot: `yarn zombienet-test -c ./examples/statemint/config.toml -t ./examples/statemint`
   - Asset Hub Kusama: `yarn zombienet-test -c ./examples/statemine/config.toml -t ./examples/statemine`
