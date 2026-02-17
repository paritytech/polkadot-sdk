# stc-shield

`stc` = **Subtensor Client**.

MEV Shield client implementation for Subtensor. Provides the concrete pieces needed to run MEV Shield on the validator side.

## What it contains

- **`MemoryShieldKeystore`** — in-memory implementation of the `ShieldKeystore` trait. Holds two ML-KEM-768 keypairs (current and next) and performs decapsulation and AEAD decryption.
- **`spawn_key_rotation_on_own_import`** — spawns a background task that rotates keys each time the validator imports one of its own blocks.
- **`InherentDataProvider`** — inserts the validator's next public key into each block as an inherent, so submitters can encrypt to it.

Requires `std`.

## How it fits in

Wire `MemoryShieldKeystore` into the node service: register it as an externalities extension, spawn the key rotation task, and include `InherentDataProvider` in the block authoring pipeline.

See [`stp-shield`](../../primitives/shield) for the trait definitions and types.
