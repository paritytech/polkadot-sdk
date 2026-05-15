# Storage Chain Tip-Sync Fixtures

This test uses a generated parachain database where old transaction-storage
entries are present in chain state but absent from the local transaction column.
The tip-sync test starts from that snapshot, warp-syncs a fresh node, renews the
old entries, and verifies the bytes are fetched through bitswap.

## Regenerating Fixtures

```bash
cd cumulus/zombienet/zombienet-sdk/tests/zombie_ci/storage_chain
./generate-snapshots.sh all
```

Useful phases:

```bash
./generate-snapshots.sh build
./generate-snapshots.sh snapshots-run
./generate-snapshots.sh snapshots-archive
./generate-snapshots.sh snapshots-test-local
```

The Rust generator writes raw chain specs and metadata into
`fixtures/test-databases`. The shell script archives the generated `pruned-node`
database into `tip-sync-100.tgz` and the relay database into `relay.tgz`.
`tip-sync-100.tgz` contains the parachain `data` and embedded relay `relay-data`
directories.

After local validation, upload the DB tarballs to the GCS paths configured in
`fixture.rs`. Do not commit the DB tarballs; keep only the raw chain specs and
metadata in the repository.
