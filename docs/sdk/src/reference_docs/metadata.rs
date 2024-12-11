//! # Metadata
//!
//! The existence of metadata in polkadot-sdk goes back to the (forkless) upgrade-ability of all
//! Substrate-based blockchains, which is achieved through
//! [`crate::reference_docs::wasm_meta_protocol`]. You can learn more about the details of how to
//! deal with these upgrades in [`crate::reference_docs::frame_runtime_upgrades_and_migrations`].
//!
//! Another consequence of upgrade-ability is that as a UI, wallet, or generally an offchain entity,
//! it is hard to know the types internal to the runtime, specifically in light of the fact that
//! they can change at any point in time.
//!
//! This is why all Substrate-based runtimes must expose a [`sp_api::Metadata`] api, which mandates
//! the runtime to return a description of itself. The return type of this api is `Vec<u8>`, meaning
//! that it is up to the runtime developer to decide on the format of this.
//!
//! All [`crate::polkadot_sdk::frame_runtime`] based runtimes expose a specific metadata language,
//! maintained in <https://github.com/paritytech/frame-metadata> which is adopted in the Polkadot
//! ecosystem.
//!
//! ## Metadata Explorers:
//!
//! A few noteworthy tools that inspect the (FRAME-based) metadata of a chain:
//!
//! - <https://wiki.polkadot.network/docs/metadata>
//! - <https://paritytech.github.io/subxt-explorer/>
