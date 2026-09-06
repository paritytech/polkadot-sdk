# CLAUDE.md — statement gossip protocol

Guidance for `sc-network-statement` and the crates it works in lockstep with:

- `substrate/client/network/statement/` — the gossip protocol (this crate).
- `substrate/client/statement-store/` — the statement store itself.
- `substrate/primitives/statement-store/` — the shared `no_std` types.

Apply this file's conventions when touching any of them.

## Layout (this crate)

- `lib.rs` — `StatementHandler` event loop and the v1 flood path (current production path).
- `affinity.rs` — `AffinityFilter`, the bloom filter a node advertises to peers.
- `v2dht/` — the new DHT path; `V2DhtOrchestrator` coordinates it.
- `config.rs` — protocol constants.

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

## DHT-affinity feature (work in progress)

The statement-store DHT-affinity feature replaces the v1 flood-gossip path with a DHT-affinity
path: a node advertises the topics it cares about and routes statements to interested peers
instead of broadcasting to everyone. The scope covers the peers topology, explicit affinity, and
peer steering modules, the orchestrator that coordinates them, store retention and configuration,
light node support, and the rollout of the protocol change. The design document and the current
task breakdown live under the umbrella issue:
https://github.com/paritytech/polkadot-sdk/issues/11932.

Remove this section when the feature stabilizes and the gate is removed.

### The v2 path is gated

`v2dht_enabled()` in `lib.rs` reads the `STATEMENT_STORE_V2_DHT_ENABLED` environment variable, off
by default. Until the feature is ready, the v2 path stays dead code in a default-configured node,
so v2 code carries `#[allow(dead_code)]`. Two invariants hold while the gate exists:

- Keep the v1 path working: with the gate off, behavior must match a node without the v2 code.
- Put every v2 call site behind `v2dht_enabled()`; never let the v2 path leak into v1 handling.

## Writing

Applies to all prose for humans — comments, doc comments, commit messages, PR descriptions, and
chat replies. Follow Strunk's *Elements of Style*:

- **Omit needless words.** Cut anything that does not change the meaning. Shorter is the default.
- **Prefer the active voice.** "The node advertises the filter," not "the filter is advertised."
  Not absolute: a noun-phrase label may take a participle ("topics advertised by this node") when
  it reads tighter than the active clause ("topics this node advertises").
- **State it positively.** Say what is, not what is not. "Empty sets match nothing," not "does not
  match anything."
- **Be concrete and specific.** Name the function, the field, the bound. Avoid "handle", "process",
  "stuff", "various".
- **Explain why, not what.** A comment earns its place by telling the reader something the code
  cannot. Never restate the code or repeat an assertion message.
- **Put the emphatic word last.** End the sentence on what matters.
- **Keep related words together** so the sentence reads in one pass.
- **One topic per paragraph**, opening with its point.

These rules serve clarity; when two of them conflict, clarity wins.

When you write or edit prose, apply these rules before you finish — do not leave a first draft.
