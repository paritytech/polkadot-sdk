---
name: Release issue template
about: Tracking issue for new releases
title: Cumulus {{ env.VERSION }} Release checklist
---
# Release Checklist

### Runtime Releases

These checks should be performed on the codebase.

- [ ] Verify [`spec_version`](#spec-version) has been incremented since the
    last release for any native runtimes from any existing use on public
    (non-private/test) networks.
- [ ] Verify previously [completed migrations](#old-migrations-removed) are
    removed for any public (non-private/test) networks.
  - No migrations added in the last release that would need to be removed.
- [ ] Verify pallet and [extrinsic ordering](#extrinsic-ordering) as well as `SignedExtension`s have stayed
    the same. Bump `transaction_version` if not.
- [ ] Verify new extrinsics have been correctly whitelisted/blacklisted for
    [proxy filters](#proxy-filtering).
  - No new extrinsics.
- [ ] Verify [benchmarks](#benchmarks) have been updated for any modified
    runtime logic.
  - [ ] Verify the weights are up-to-date.
- [ ] Verify that the various pieces of XCM config are sane.

The following checks can be performed after we have forked off to the release-
candidate branch or started an additional release candidate branch (rc-2, rc-3, etc)

- [ ] Verify [new migrations](#new-migrations) complete successfully, and the
    runtime state is correctly updated for any public (non-private/test)
    networks.
- [ ] Run integration tests.
  - [ ] Teleport Relay -> Statemin* and back.
  - [ ] Create asset (if applicable), mint and transfer
- [ ] Push runtime upgrade to Westmint and verify network stability.


### All Releases

- [ ] Check that the new polkadot-parachain versions have [run on the network](#burn-in)
    without issue.
- [ ] Check that a draft release has been created at
    https://github.com/paritytech/cumulus/releases with relevant [release
    notes](#release-notes).
- [ ] Check that [build artifacts](#build-artifacts) have been added to the
    draft-release.

---

## Notes

### Burn In

Ensure that Parity DevOps has run the new release on Westmint and Statemine collators for 12h prior to publishing the release.

### Build Artifacts

Add any necessary assets to the release. They should include:

- Linux binary
- GPG signature of the Linux binary
- SHA256 of binary
- Source code
- Wasm binaries of any runtimes

### Release notes

The release notes should list:

- The priority of the release (i.e., how quickly users should upgrade) - this is
    based on the max priority of any *client* changes.
- Which native runtimes and their versions are included
- The proposal hashes of the runtimes as built with
    [srtool](https://github.com/paritytech/srtool)
- Any changes in this release that are still awaiting audit

The release notes may also list:

- Free text at the beginning of the notes mentioning anything important
    regarding this release
- Notable changes separated into sections.

### Spec Version

A runtime upgrade must bump the spec number. This may follow a pattern with the
client release (e.g. runtime v12 corresponds to v0.8.12, even if the current
runtime is not v11).

### Runtime version bump between RCs

The clients need to be aware of runtime changes. However, we do not want to bump the
`spec_version` for every single release candidate. Instead, we can bump the `impl` field of the version
to signal the change to the client.

### Old Migrations Removed

Previous `on_runtime_upgrade` functions from old upgrades should be removed.

### New Migrations

Ensure that any migrations that are required due to storage or logic changes
are included in the `on_runtime_upgrade` function of the appropriate pallets.

### Extrinsic Ordering & Storage

Offline signing libraries depend on a consistent ordering of call indices and
functions. Compare the metadata of the current and new runtimes and ensure that
the `module index, call index` tuples map to the same set of functions. It also checks if there have been any changes in `storage`. In case of a breaking change, increase `transaction_version`.

To verify the order has not changed, manually start the following [Github Action](https://github.com/paritytech/cumulus/actions/workflows/extrinsic-ordering-check-from-bin.yml). It takes around a minute to run and will produce the report as artifact you need to manually check.

To run it, in the _Run Workflow_ dropdown:
1. **Use workflow from**: to ignore, leave `master` as default
2. **The WebSocket url of the reference node**:
    - Statemine: `wss://kusama-statemine-rpc.paritytech.net`
    - Westmint: `wss://westmint-rpc.polkadot.io`
3. **A url to a Linux binary for the node containing the runtime to test**: Paste the URL of the latest release-candidate binary from the draft-release on Github. The binary has to previously be uploaded to S3 (Github url link to the binary is constantly changing)
    - https://releases.parity.io/cumulus/statemine-v6.0.0-rc1/polkadot-collator
4. **The name of the chain under test. Usually, you would pass a local chain**:
    - Statemine: `statemine-local`
    - Westmint: `westmint-local`
5. Click **Run workflow**

When the workflow is done, click on it and download the zip artifact, inside you'll find an `output.txt` file. The things to look for in the output are lines like:

- `[Identity] idx 28 -> 25 (calls 15)` - indicates the index for Identity has changed
- `[+] Society, Recovery` - indicates the new version includes 2 additional modules/pallets.
- If no indices have changed, every modules line should look something like `[Identity] idx 25 (calls 15)`

**Note**: Adding new functions to the runtime does not constitute a breaking change
as long as the indexes did not change.

**Note**: Extrinsic function signatures changes (adding/removing & ordering arguments) are not caught by the job, so those changes should be reviewed "manually"

### Proxy Filtering

The runtime contains proxy filters that map proxy types to allowable calls. If
the new runtime contains any new calls, verify that the proxy filters are up to
date to include them.

### Benchmarks

Until #631 is done, running the benchmarks is a manual process:
1. Connect to the bechmarking machine
2. Make sure no one else is using the machine with `htop check`
3. Pull in the branch of Cumulus that has the version of Statemine you want to release
4. Recompile `cargo build --profile production --locked --features runtime-benchmarks`
5. From the root directory run `nohup ./scripts/benchmarks.sh &` (it will take quite a few hours)
6. Checkout in your local machine to the branch of cumulus that has the version of Statemine you want to release
7. `scp` from the host to your local machine the weights for Statemine, Westmint and Statemint you'll find in:
   - `/parachains/runtimes/assets/statemine/src/weights`
   - `/parachains/runtimes/assets/statemint/src/weights`
   - `/parachains/runtimes/assets/westmint/src/weights`
8. Commit the changes in your local and create a PR
