# bitswap zombienet tests

E2e coverage of the `bitswap_unstable_*` JSON-RPC namespace, plus the generator that produces
the bulletin DB snapshots the e2e test runs against.

## Layout

| File | Role |
| --- | --- |
| `payloads.rs` | Deterministic bulletin payloads + their CIDs, shared by both tests (ported from smoldot `e2e-tests/src/bulletin.rs`). A unit test pins that the CIDs reproduce the values smoldot serves. |
| `common.rs` | Shared network config (`network_config`, one topology for both tests) + chain-spec resolution (`resolve_chain_spec`). |
| `e2e.rs` | The consumer test `bitswap_unstable_e2e` (`#[ignore]`d). |
| `generate_snapshot.rs` | The snapshot generator `bitswap_generate_snapshot` (`#[ignore]`d). |

Both tests are `#[ignore]`d (they need zombienet binaries), so they compile in normal `cargo check`
but only run when explicitly requested with `--ignored`.

## Chain-spec provenance

The bulletin runtime is **not** in polkadot-sdk. The parachain chain-spec is referenced by URL
(`common.rs::CHAIN_SPEC_BULLETIN`) from smoldot's checked-in copy,
`paritytech/smoldot:e2e-tests/chain-specs/bulletin-westend-local-spec.json`, pinned to a commit.
That file is itself generated upstream by
[`polkadot-bulletin-chain/scripts/create_bulletin_westend_spec.sh`](https://github.com/paritytech/polkadot-bulletin-chain/blob/main/scripts/create_bulletin_westend_spec.sh).
Override it with `BULLETIN_CHAIN_SPEC_OVERRIDE=/path/or/url` when iterating on a newer runtime.

## Generating snapshots

Prerequisites: `polkadot` and `polkadot-parachain` on `$PATH`.

```sh
cargo test -p cumulus-zombienet-sdk-tests --features zombie-ci \
    --test tests -- --ignored bitswap_generate_snapshot --nocapture
```

Outputs land in `$BITSWAP_SNAPSHOT_OUT_DIR` (default `target/snapshots/`):
`relaychain.tgz` and `bulletin-full.tgz`. The run prints the exact upload commands.

## Uploading + wiring the consumer

Snapshots are hosted under `gs://zombienet-db-snaps/cumulus/bitswap/` with a git-revision suffix
(matching the `cumulus/0007-full_node_warp_sync/` convention). Upload manually:

```sh
REV=$(git rev-parse --short HEAD)
gcloud storage cp target/snapshots/relaychain.tgz    gs://zombienet-db-snaps/cumulus/bitswap/relaychain-$REV.tgz
gcloud storage cp target/snapshots/bulletin-full.tgz gs://zombienet-db-snaps/cumulus/bitswap/bulletin-full-$REV.tgz
```

Then set `SNAPSHOT_REV = "$REV"` in `e2e.rs` so the consumer points at the freshly uploaded
artifacts. Regeneration is only needed when the bulletin runtime or `payloads()` changes.

For local iteration without uploading, point the consumer at the local tarballs:

```sh
export DB_SNAPSHOT_RELAY_OVERRIDE=target/snapshots/relaychain.tgz
export DB_SNAPSHOT_BULLETIN_FULL_OVERRIDE=target/snapshots/bulletin-full.tgz
```
