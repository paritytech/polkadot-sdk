pub mod converter;
pub mod message;
pub mod message_receipt;

pub use converter::*;
pub use message::*;
pub use message_receipt::*;

use codec::{Encode, Decode};
use scale_info::TypeInfo;
use sp_runtime::RuntimeDebug;
use sp_core::H160;
use sp_std::prelude::*;

#[derive(Copy, Clone, Encode, Decode, Eq, PartialEq, Debug, TypeInfo)]
pub enum DryRunError {
	ConvertLocationFailed,
	ConvertXcmFailed,
}

#[derive(Clone, Encode, Decode, PartialEq, RuntimeDebug, TypeInfo)]
pub struct TransactInfo {
	pub target: H160,
	pub data: Vec<u8>,
	pub gas_limit: u64,
	pub value: u128,
}
