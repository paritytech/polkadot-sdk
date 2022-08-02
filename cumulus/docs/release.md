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

Ensure that Parity DevOps has run the new release on Westmint and Statemine collators for 12h prior to publishing the
release.

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

Offline signing libraries depend on a consistent API of call indices and functions. Nowadays, we are no longer using
indices but the task of checking whether the API remains compatible remained and the name "Extrinsic Ordering" remained
as well although we no longer check the ordering itself.

Compare the metadata of the current and new runtimes and ensure that the `module index, call index` tuples map to the
same set of functions. It also checks if there have been any changes in `storage`. In case of a breaking change,
increase `transaction_version`.

To verify that the API did not break, manually start the following [Github
Action](https://github.com/paritytech/cumulus/actions/workflows/extrinsic-ordering-check-from-bin.yml). It takes around
a minute to run and will produce the report as artifact you need to manually check.

When the workflow is done, click on it and download the zip artifact, inside you'll find an `output.txt` file. The
output should be appended as comment to the "Checklist issue".

The things to look for in the output are lines like:

- `[Identity] idx 28 -> 25 (calls 15)` - indicates the index for Identity has changed
- `[+] Society, Recovery` - indicates the new version includes 2 additional modules/pallets
- If no indices have changed, every modules line should look something like `[Identity] idx 25 (calls 15)`

**Note**: Adding new functions to the runtime does not constitute a breaking change as long as the indexes did not
change.

**Note**: Extrinsic function signatures changes (adding/removing & ordering arguments) are not caught by the job, so
those changes should be reviewed "manually"

### Benchmarks

The Benchmarks can now be started from the CI. First find the CI pipeline from
[here](https://gitlab.parity.io/parity/mirrors/cumulus/-/pipelines?page=1&scope=all&ref=release-parachains-v9220) and
pick the latest.
