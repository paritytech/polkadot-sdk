# stc-shield

`stc` = **Subtensor Client**.

MEV Shield client implementation for Subtensor. Provides the concrete pieces needed to run MEV Shield on the validator side.

## What it contains

- **`MemoryShieldKeystore`** — in-memory implementation of the `ShieldKeystore` trait. Holds two ML-KEM-768 keypairs (current and next) and exposes the raw key bytes for the runtime to perform decryption in WASM.
- **`spawn_key_rotation_on_own_import`** — spawns a background task that rotates keys (`roll_for_next_slot`) each time the validator imports one of its own blocks. This promotes the `next` keypair to `current` and generates a fresh `next` keypair.
- **`InherentDataProvider`** — inserts the validator's next encapsulation key into each block as an inherent, so submitters can encrypt to it.

Requires `std`.

## How it fits in

Wire `MemoryShieldKeystore` into the node service: pass it to the proposer factory and the inherent data provider, and spawn the key rotation task. During block authoring, the proposer uses `is_shielded_using_current_key` (via the runtime API) to check whether a shielded transaction's `key_hash` matches the key it can decrypt, then calls `current_dec_key()` and passes it to the runtime API for in-WASM decryption.

See [`stp-shield`](../../primitives/shield) for the trait definitions and types.
