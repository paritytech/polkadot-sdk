//! Hardcoded assumptions of this omni-node.

// TODO: is this actually simpler than using `Multi*` stuff?
pub type AccountId = sp_runtime::AccountId32;
pub type Nonce = u32;
pub type BlockNumber = u32;
pub type Hashing = sp_runtime::traits::BlakeTwo256;
pub type Header = sp_runtime::generic::Header<BlockNumber, Hashing>;
pub type OpaqueBlock = sp_runtime::generic::Block<Header, sp_runtime::OpaqueExtrinsic>;
