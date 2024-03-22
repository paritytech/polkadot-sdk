//! TOOD:

pub type Nonce = u32;
pub type Balance = u128;
pub type Header = <frame::runtime::types_common::OpaqueBlock as sp_runtime::traits::Block>::Header;
pub use frame::runtime::types_common::{AccountId, BlockNumber, OpaqueBlock};
