//! # Enable metadata hash verification
//!
//! This guide will teach you how to enable the metadata hash verification in your runtime.
//!
//! ## What is metadata hash verification?
//!
//! Each FRAME based runtime exposes metadata about itself. This metadata is used by consumers of
//! the runtime to interpret the state, to construct transactions etc. Part of this metadata are the
//! type information. These type information can be used to e.g. decode storage entries or to decode
//! a transaction. So, the metadata is quite useful for wallets to interact with a FRAME based
//! chain. Online wallets can fetch the metadata directly from any node of the chain they are
//! connected to, but offline wallets can not do this. So, for the offline wallet to have access to
//! the metadata it needs to be transferred and stored on the device. The problem is that the
//! metadata has a size of several hundreds of kilobytes, which takes quite a while to transfer to
//! these offline wallets and the internal storage of these devices is also not big enough to store
//! the metadata for one or more networks. The next problem is that the offline wallet/user can not
//! trust the metadata to be correct. It is very important for the metadata to be correct or
//! otherwise an attacker could change them in a way that the offline wallet decodes a transaction
//! in a different way than what it will be decoded to on chain. So, the user may signs an incorrect
//! transaction leading to unexpecting behavior.
//!
//! The metadata hash verification circumvents the issues of the huge metadata and the need to trust
//! some metadata blob to be correct. To generate a hash for the metadata, the metadata is chunked,
//! these chunks are put into a merkle tree and then the root of this merkle tree is the "metadata
//! hash". For a more technical explanation on how it works, see
//! [RFC78](https://polkadot-fellows.github.io/RFCs/approved/0078-merkleized-metadata.html). At compile
//! time the metadata hash is generated and "backed" into the runtime. This makes it extremely cheap
//! for the runtime to verify on chain that the metadata hash is correct. By having the runtime
//! verify the hash on chain, the user also doesn't need to trust the offchain metadata. If the
//! metadata hash doesn't match the on chain metadata hash the transaction will be rejected. The
//! metadata hash itself is added to the data of the transaction that is signed, this means the
//! actual hash does not appear in the transaction. On chain the same procedure is repeated with the
//! metadata hash that is known by the runtime and if the metadata hash doesn't match the signature
//! verification will fail. As the metadata hash is actually the root of a merkle tree, the offline
//! wallet can get proofs of individual types to decode a transaction. This means that the offline
//! wallet does not require the entire metadata to be present on the device.
//!
//! ## Integrating metadata hash verification into your runtime
//!
//! The integration of the metadata hash verification is split into two parts, first the actual
//! integration into the runtime and secondly the enabling of the metadata hash generation at
//! compile time.
//!
//! ### Runtime integration
//!
//! From the runtime side only the
//! [`CheckMetadataHash`](frame_metadata_hash_extension::CheckMetadataHash) needs to be added to the
//! list of signed extension:
#![doc = docify::embed!("../../templates/parachain/runtime/src/lib.rs", template_signed_extra)]
//!
//! > **Note:**
//! >
//! > Adding the signed extension changes the encoding of the transaction and adds one extra byte
//! > per transaction!
//!
//! This signed extension will make sure to decode the requested `mode` and will add the metadata
//! hash to the signed data depending on the requested `mode`. The `mode` gives the user/wallet
//! control over deciding if the metadata hash should be verified or not. The metadata hash itself
//! is drawn from the `RUNTIME_METADATA_HASH` environment variable. If the environment variable is
//! not set, any transaction that requires the metadata hash is rejected with the error
//! `CannotLookup`. This is a security measurement to prevent including invalid transactions.
//!
//! <div class="warning">
//!
//! The extension does not work with the native runtime, because the
//! `RUNTIME_METADATA_HASH` environment variable is not set when building the
//! `frame-metadata-hash-extension` crate.
//!
//! </div>
//!
//! ### Enable metadata hash generation
//!
//! The metadata hash generation needs to be enabled when building the wasm binary. The
//! `substrate-wasm-builder` supports this out of the box:
#![doc = docify::embed!("../../templates/parachain/runtime/build.rs", template_enable_metadata_hash)]
//!
//! > **Note:**
//! >
//! > The `metadata-hash` feature needs to be enabled for the `substrate-wasm-builder` to enable the
//! > code for being able to generate the metadata hash. It is also recommended to put the metadata
//! > hash generation behind a feature in the runtime as shown above. The reason behind is that it
//! > adds a lot of code which increases the compile time and the generation itself also increases
//! > the compile time. Thus, it is recommended to enable the feature only when the metadata hash is
//! > required (e.g. for an on-chain build).
//!
//! The two parameters to `enable_metadata_hash` are the token symbol and the number of decimals of
//! the primary token of the chain. These information are included for the wallets to show token
//! related operations in a more user friendly way.
