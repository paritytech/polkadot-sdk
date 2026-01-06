# Full Node Warp Sync Test

Integration test verifying warp sync functionality for relay chain and parachain nodes using Zombienet.

## Updating Test Artifacts

Test maintenance involves updating chain specs and database snapshots after runtime changes to `cumulus-test-runtime`
or `rococo-local`.

### Using `generate-snapshots.sh` script

Use the automation script for all steps:

```bash
cd cumulus/zombienet/zombienet-sdk/tests/zombie_ci/full_node_warp_sync

# Run all phases
./generate-snapshots.sh all

# Or run individual phases
./generate-snapshots.sh build                   # Build binaries
./generate-snapshots.sh chainspec-parachain     # Generate parachain spec
./generate-snapshots.sh chainspec-relaychain    # Generate relaychain spec
./generate-snapshots.sh snapshots-generate      # Run snapshot generation
./generate-snapshots.sh snapshots-archive       # Create snapshot tarballs
./generate-snapshots.sh snapshots-test-local    # Validate with local snapshots
```

Once the snapshots are validated they are ready to upload to google storage.

## Testing local snapshots

```bash
DB_SNAPSHOT_RELAYCHAIN_OVERRIDE=$(realpath relaychain-db.tgz) \
DB_SNAPSHOT_PARACHAIN_OVERRIDE=$(realpath parachain-db.tgz) \
TARGET_DIR=$(dirname "$(cargo locate-project --workspace --message-format plain)")/target/release \
PATH="$TARGET_DIR:$PATH" \
RUST_LOG=info,zombienet_orchestrator=debug \
ZOMBIE_PROVIDER=native \
cargo nextest run --release \
    -p cumulus-zombienet-sdk-tests \
    --features zombie-ci \
    --no-capture \
    -- full_node_warp_sync
```

Exactly these steps are performed with command:
```bash
./generate-snapshots.sh snapshots-test-local
```
