# CLAUDE.md — statement-store DHT feature (work in progress)

**While this file exists, work across the statement-store crates is part of the statement-store
DHT-affinity feature: https://github.com/paritytech/polkadot-sdk/issues/11932** The feature spans:

- `substrate/client/network/statement/` — the gossip protocol (this crate).
- `substrate/client/statement-store/` — the statement store itself.
- `substrate/primitives/statement-store/` — the shared `no_std` types.

Apply this file's conventions when touching any of them.

**Before merging `feature/statement-store-dht` into `master`, remove this file and any other
AI-only helper files added for this work** — they must not reach `master`.

## Branching

- `feature/statement-store-dht` is the integration branch for this feature.
- Base every new PR on `feature/statement-store-dht`, not `master`, and target it in the PR.

## What the feature does

Replaces the v1 flood-gossip path with a DHT-affinity path: a node advertises the topics it cares
about and routes statements to interested peers instead of broadcasting to everyone. Built from a
design document and split across coordinated sub-issues:

- #11932 — umbrella feature
- #11933 — peers topology module (which peers to keep connected, by affinity)
- #11934 — explicit affinity module (which topics this node cares about)
- #11935 — peer steering module
- #11936 — store limitations and configuration
- #11937 — statement-store orchestrator
- #10910 — refactor index locking
- #11938 — rollout plan for the protocol change
- #11288 — light node support

## Layout (this crate)

- `lib.rs` — `StatementHandler` event loop and the v1 flood path (current production path).
- `affinity.rs` — `AffinityFilter`, the bloom filter a node advertises to peers.
- `v2dht/` — the new DHT path; `V2DhtOrchestrator` coordinates it.
- `config.rs` — protocol constants.

## The v2 path is gated

`v2dht_enabled()` in `lib.rs` is hard-coded `false`. The new path is dead code behind it until the
feature is ready, so v2 code carries `#[allow(dead_code)]`. Keep the v1 path working meanwhile.

## Wire compatibility

`AffinityFilter`'s encoding is a network contract shared with other implementations, including the
light client. `affinity_filter_encoding_snapshot` pins the exact bytes — treat the wire format as
stable and change it only deliberately, in step with the other side.

## Build & test

```bash
SKIP_WASM_BUILD=1 cargo test -p sc-network-statement
SKIP_WASM_BUILD=1 cargo clippy -p sc-network-statement --all-targets
cargo +nightly fmt -p sc-network-statement
```

## Writing

Applies to all prose for humans — comments, doc comments, commit messages, PR descriptions, and
chat replies. Follow Strunk's *Elements of Style*:

- **Omit needless words.** Cut anything that does not change the meaning. Shorter is the default.
- **Use the active voice.** "The node advertises the filter," not "the filter is advertised."
- **State it positively.** Say what is, not what is not. "Empty sets match nothing," not "does not
  match anything."
- **Be concrete and specific.** Name the function, the field, the bound. Avoid "handle", "process",
  "stuff", "various".
- **Explain why, not what.** A comment earns its place by telling the reader something the code
  cannot. Never restate the code or repeat an assertion message.
- **Put the emphatic word last.** End the sentence on what matters.
- **Keep related words together** so the sentence reads in one pass.
- **One topic per paragraph**, opening with its point.

When you write or edit prose, apply these rules before you finish — do not leave a first draft.
