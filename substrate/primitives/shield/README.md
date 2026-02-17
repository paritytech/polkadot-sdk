# stp-shield

`stp` = **Subtensor Primitive**.

MEV Shield primitives for the Subtensor runtime. Defines the shared interface between the runtime and the client for transaction privacy via encryption.

## What it contains

- **`ShieldKeystore` trait** — interface for keystore operations: key rotation, public key retrieval, ML-KEM-768 decapsulation, and XChaCha20-Poly1305 decryption.
- **`ShieldedTransaction`** — structure representing an encrypted transaction (KEM ciphertext + AEAD ciphertext + nonce).
- **`ShieldApi` runtime API** — allows the runtime to decode and decrypt shielded extrinsics.
- **`ShieldKeystoreExt`** — registers the keystore as a Substrate externalities extension for use inside the runtime.
- Common constants and type aliases (`ShieldPublicKey`, `InherentType`, `INHERENT_IDENTIFIER`).

`no_std`-compatible.

## How it fits in

Validators announce a public key via a block inherent. Submitters encrypt their transactions to that key. The validator decrypts them when building the block, preventing front-running.

See [`stc-shield`](../../client/shield) for the client-side implementation.
