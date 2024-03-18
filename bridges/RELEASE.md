## How to Release a New Version of Relayer

This document has the reverse order - the steps required to make a release
come first and details come in the last sections.

### Making a Release

All releases are supposed to be done from the
[`polkadot-staging` branch](https://github.com/paritytech/parity-bridges-common/tree/polkadot-staging).
This branch is assumed to contain changes, that are reviewed and audited.

To prepare a release:

1. Make sure all required changes are merged to the
  [`polkadot-staging` branch](https://github.com/paritytech/parity-bridges-common/tree/polkadot-staging);

2. Select release version: go to the `Cargo.toml` of `substrate-relay` crate
  ([here](https://github.com/paritytech/parity-bridges-common/blob/polkadot-staging/relays/bin-substrate/Cargo.toml#L3))
  to look for the latest version. Then increment the minor or major version.

    **NOTE**: we are not going to properly support [semver](https://semver.org)
    for the relayer. When we simply bump a chain versions or introduce small fixes,
    minor version must be incremented. If changes are substantial (e.g. when
    we'll release relayer for [bridges v2](https://github.com/paritytech/parity-bridges-common/milestone/17))
    it'll make sense to increment major version;

3. Open a PR with a `substrate-relay` version change (see previous point).
  It could be combined with the (1) if changes are not large. Make sure to
  add the [`A-release`](https://github.com/paritytech/parity-bridges-common/labels/A-release)
  label to your PR - in the future we'll add workflow to make pre-releases
  when such PR is merged to the `polkadot-staging` branch;

4. Wait for approvals and merge PR, mentioned in (3);

5. Checkout updated `polkadot-staging` branch and do `git pull`;

6. Make a new git tag with the `substrate-relay` version:
```sh
git tag -a v1.5.0 -m "Release v1.5.0"
git push origin v1.5.0
```

7. Pushing that tag triggers a new pipeline at
  [GitLab](https://gitlab.parity.io/parity/mirrors/parity-bridges-common/-/pipelines).
  Wait until until that pipeline succeeds. Make sure relayer docker image is published
  to the [docker hub](https://hub.docker.com/r/paritytech/substrate-relay/tags);

8. Go to the [`New Release`](https://github.com/paritytech/parity-bridges-common/releases/new)
  section of the repository. Make sure to:

  - set release name to "Release vX.Y.Z" (example: "Release v1.5.0");

  - at the beginning of the Release Description add the important notes if needed;

  - right below that, add the reference to the relayer docker image:
```
Docker reference: paritytech/substrate-relay:v1.5.0
```

  - (**IMPORTANT**) prepare the list of bundled chain versions and add it right after the
    docker reference. Example:
```
Bundled Chain Versions:

- Rococo: `1_003_000`;

- Westend: `1_003_000`;

- Kusama: `9410`;

- Polkadot: `9410`;

- Rococo Bridge Hub: `9410`;

- Westend Bridge Hub: `9410`;

- Kusama Bridge Hub: `9410`;

- Polkadot Bridge Hub: `9410`;

- Rococo Bulletin: `None` (must be specified in CLI);

- Polkadot Bulletin: `None` (must be specified in CLI).
```

  - choose new and previous tags and hit the "Generate Release Notes" button;

  - hit the "Publish Release" button.

### When to Make a New Release

The relayer from this repository supports multiple bridges:

- `Rococo Bridge Hub` (aka `RBH`) <> `Westend Bridge Hub` (aka `WBH`) bridge;

- `Rococo Bridge Hub` <> `Rococo Bulletin Chain` (aka `RBC`) bridge;

- `Kusama Bridge Hub` (aka `KBH`) <> `Polkadot Bridge Hub` (aka `PBH`) bridge;

- `Polkadot Bridge Hub` <> `Polkadot Bulletin Chain` (aka `PBC`) bridge.

We run every relayer in two modes: one is to relay messages and associated finality
proofs (it is usually `relay-headers-and-messages` subcommand) and the other is
the equivocation detection relayer (`detect-equivocations` command). The relayer
working in the former mode, submits transactions only to the chains it directly connects.
Relayer, running in the latter mode, submits transactions to relay chains (e.g. to
`Polkadot` or to `Kusama`).

To submit transaction to some chain, relayer would need to know how to construct and
properly encode this transaction. In current implementation, this information is
hardcoded in the relayer code. This information may change from release to release,
so we need to make a new relayer release once one of changes is upgraded.

However, we are cheating here - for test bridges (`RBH` <> `WBH` and `RBH` <> `RBC`)
we are running relayer in a mode, when it just uses this hardcoded information,
ignoring actual runtime version. So normally we'll made releases only when following
chain runtimes are changes: `Polkadot`, `Kusama`, `PBH`, `KBH`.

### Adding Support for Updated Chain

When one of involved chains is upgraded, we need to update the relayer code to
support it. Normally it means:

1. Bumping bundled chain versions in following places:

- for `Rococo` and `RBH`: [here](https://github.com/paritytech/parity-bridges-common/blob/polkadot-staging/relays/bin-substrate/src/chains/rococo.rs);

- for `Westend` and `WBH`: [here](https://github.com/paritytech/parity-bridges-common/blob/polkadot-staging/relays/bin-substrate/src/chains/westend.rs);

- for `Kusama` and `KBH`: [here](https://github.com/paritytech/parity-bridges-common/blob/polkadot-staging/relays/bin-substrate/src/chains/polkadot.rs)

- for `Polkadot` and `PBH`: [here](https://github.com/paritytech/parity-bridges-common/blob/polkadot-staging/relays/bin-substrate/src/chains/polkadot.rs);

- for `PBC`: [here](https://github.com/paritytech/parity-bridges-common/blob/polkadot-staging/relays/bin-substrate/src/chains/polkadot_bulletin.rs).

2. Regenerating bundled runtime wrapper code using `runtime-codegen` binary:

If you can start updated chain node, it could be done using following command
(assuming you're in the root of the repository):
```sh
cd tools/runtime-codegen
cargo run --bin runtime-codegen -- --from-node-url "wss://rococo-rpc.polkadot.io:443" > ../../relays/client-rococo/src/codegen_runtime.rs
```

Otherwise, you'll need a runtime file. You may download it from:

- [releases page](https://github.com/paritytech/polkadot-sdk/releases) of `polkadot-sdk`
  for `Rococo`, `Westend`, `RBH` and `WBH`;

- [releases page](https://github.com/polkadot-fellows/runtimes/releases) of `runtimes`
  for `Kusama`, `Polkadot`, `KBH` and `PBH`.

Then use the following command:
```sh
cd tools/runtime-codegen
cargo run --bin runtime-codegen -- --from-wasm-file rococo_runtime.compact.compressed.wasm > ../../relays/client-rococo/src/codegen_runtime.rs
```

**IMPORTANT**: due to [well-known issue](https://github.com/paritytech/parity-bridges-common/issues/2669)
with runtime codegen, you'll get compilation errors after updating.
To fix it, execute following commands:
```sh
cargo +nightly fmt --all
find . -name codegen_runtime.rs -exec \
    sed -i 's/::sp_runtime::generic::Header<::core::primitive::u32>/::sp_runtime::generic::Header<::core::primitive::u32, ::sp_runtime::traits::BlakeTwo256>/g' {} +
```
