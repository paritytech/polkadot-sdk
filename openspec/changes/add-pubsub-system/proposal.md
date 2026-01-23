# Change: Add Pub-Sub System for Cross-Chain Data Sharing

## Why

Parachains currently lack a native mechanism to share arbitrary data with other parachains via the relay chain. While XCM enables message passing, there is no built-in publish-subscribe pattern where publishers can store data on the relay chain and subscribers can efficiently retrieve it with cryptographic proofs. This limits cross-chain applications like shared oracles, ring signature roots (POP), and configuration synchronization.

RFC-0160 defines a pub-sub mechanism allowing parachains to publish key-value data to the relay chain, which subscribers can then access via relay state proofs included in their validation data.

## What Changes

### Broadcaster Pallet (Relay Chain)
- Publisher registration with configurable deposits for public parachains
- System parachains (ID < 2000) can publish without registration
- Per-publisher child trie storage with configurable limits
- TTL-based automatic data expiration via `on_idle`
- Manual deletion APIs for parachains and governance

### Subscriber Pallet (Parachains)
- `SubscriptionHandler` trait for declaring subscriptions
- `SubscribedKey` type with compile-time hashing via `subscribed_key!` macro or runtime hashing via `SubscribedKey::from_raw_key()`
- Change detection using child trie roots to skip unchanged publishers
- Trie node caching for PoV size reduction
- Budget-constrained proof processing with cursor-based resumption

### XCM Publish Instruction
- New `Publish { key, value, ttl }` instruction in XCM v5
- Single key-value pair per instruction (batch via multiple instructions)
- TTL support: 0 = infinite, N = expire after N blocks

### PoV Optimization
- Proof pruning in `provide_inherent` using on-chain trie node cache
- Budget allocation: pub-sub uses remaining space after messages
- Cache synchronization via dual-trie traversal

## Impact

- Affected specs: None (new capability)
- Affected code:
  - `polkadot/runtime/parachains/src/broadcaster/` (new)
  - `cumulus/pallets/subscriber/src/lib.rs` (new)
  - `polkadot/xcm/src/v5/instruction.rs` (XCM Publish)
  - `polkadot/xcm/xcm-executor/src/lib.rs` (executor integration)
  - `cumulus/client/parachain-inherent/src/lib.rs` (proof collection)
  - `cumulus/pallets/parachain-system/src/lib.rs` (proof pruning)
  - `cumulus/primitives/core/src/lib.rs` (types, runtime APIs)
