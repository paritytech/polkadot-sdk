//! Hardcoded assumptions of this omni-node.
//!
//! Consensus: This template uses [`sc-manual-seal`] consensus and therefore has no expectation of
//! the runtime having any consensus-related pallets. The block time of the node can easily be
//! adjusted by [`crate::cli::Cli::consensus`]

/// The account id type that is expected to be used in `frame-system`.
// TODO: is this actually simpler than using `Multi*` stuff?
pub type AccountId = sp_runtime::AccountId32;
/// The index type that is expected to be used in `frame-system`.
pub type Nonce = u32;
/// The block type that is expected to be used in `frame-system`.
pub type BlockNumber = u32;
/// The hash type that is expected to be used in `frame-system`.
pub type Hashing = sp_runtime::traits::BlakeTwo256;

/// The hash type that is expected to be used in the runtime.
pub type Header = sp_runtime::generic::Header<BlockNumber, Hashing>;
/// The opaque block type that is expected to be used in the runtime.
pub type OpaqueBlock = sp_runtime::generic::Block<Header, sp_runtime::OpaqueExtrinsic>;
