use super::*;
use sp_runtime::{generic, traits::BlakeTwo256};

pub use sp_runtime::OpaqueExtrinsic as UncheckedExtrinsic;
/// Opaque block header type.
pub type Header = generic::Header<BlockNumber, BlakeTwo256>;
/// Opaque block type.
pub type Block = generic::Block<Header, UncheckedExtrinsic>;
/// Opaque block identifier type.
pub type BlockId = generic::BlockId<Block>;
