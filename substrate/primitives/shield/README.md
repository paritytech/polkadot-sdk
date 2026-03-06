# stp-shield

`stp` = **Subtensor Primitive**.

MEV Shield primitives for the Subtensor runtime. Defines the shared interface between the runtime and the client for transaction privacy via encryption.

## What it contains

- **`ShieldKeystore` trait** — interface for keystore operations: key rotation (`roll_for_next_slot`), encapsulation key retrieval (`next_enc_key`), and decapsulation key retrieval (`current_dec_key`).
- **`ShieldedTransaction`** — structure representing an encrypted transaction (KEM ciphertext + AEAD ciphertext + nonce).
- **`ShieldApi` runtime API** — allows the node to call into the runtime to decode and decrypt shielded extrinsics. The decapsulation key is passed as a parameter so no host functions are needed.
- Common constants and type aliases (`ShieldEncKey`, `MLKEM768_ENC_KEY_LEN`, `InherentType`, `INHERENT_IDENTIFIER`).

`no_std`-compatible.

## How it fits in

Validators announce an ML-KEM-768 encapsulation key via a block inherent. Submitters encrypt their transactions to that key. The validator passes the decapsulation key to the runtime API when building the block, and the runtime decrypts them in pure WASM — preventing front-running without requiring host functions.

See [`stc-shield`](../../client/shield) for the client-side implementation.
