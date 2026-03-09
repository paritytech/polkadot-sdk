# stp-shield

`stp` = **Subtensor Primitive**.

MEV Shield primitives for the Subtensor runtime. Defines the shared interface between the runtime and the client for transaction privacy via encryption.

## What it contains

- **`ShieldKeystore` trait** — interface for keystore operations: key rotation (`roll_for_next_slot`), encapsulation key retrieval (`next_enc_key`), and decapsulation key retrieval (`current_dec_key`).
- **`ShieldedTransaction`** — structure representing an encrypted transaction (`key_hash` + KEM ciphertext + nonce + AEAD ciphertext).
- **`ShieldApi` runtime API** — allows the node to call into the runtime to decode and decrypt shielded extrinsics. Includes `try_decode_shielded_tx` (parse a shielded wrapper) and `is_shielded_using_current_key` (check if a transaction's `key_hash` matches the key the proposer can decrypt). The decapsulation key is passed as a parameter so no host functions are needed.
- Common constants and type aliases (`ShieldEncKey`, `MLKEM768_ENC_KEY_LEN`, `InherentType`, `INHERENT_IDENTIFIER`).

`no_std`-compatible.

## Key lifecycle

The keystore holds two ML-KEM-768 keypairs: **current** and **next**.

- `next_enc_key()` returns the encapsulation key of the `next` pair — this is announced via the block inherent so users can encrypt to it.
- `current_dec_key()` returns the decapsulation key of the `current` pair — used by the proposer to decrypt shielded transactions in the block being built.
- `roll_for_next_slot()` promotes `next` to `current` and generates a fresh `next` pair. Called when the validator imports one of its own blocks.

## How it fits in

Validators announce an ML-KEM-768 encapsulation key via a block inherent. Submitters encrypt their transactions to that key. The validator passes the decapsulation key to the runtime API when building the block, and the runtime decrypts them in pure WASM — preventing front-running without requiring host functions.

See [`stc-shield`](../../client/shield) for the client-side implementation.
