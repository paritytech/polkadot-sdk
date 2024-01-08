use codec::{Decode, Encode, MaxEncodedLen};
use ethabi::Token;
use frame_support::traits::ProcessMessage;
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};
use sp_arithmetic::FixedU128;
use sp_core::H256;
use sp_runtime::{traits::Zero, RuntimeDebug};
use sp_std::prelude::*;

use super::Pallet;

use snowbridge_core::ChannelId;
pub use snowbridge_outbound_queue_merkle_tree::MerkleProof;

pub type ProcessMessageOriginOf<T> = <Pallet<T> as ProcessMessage>::Origin;

pub const LOG_TARGET: &str = "snowbridge-outbound-queue";

/// Message which has been assigned a nonce and will be committed at the end of a block
#[derive(Encode, Decode, Clone, PartialEq, RuntimeDebug, TypeInfo)]
pub struct CommittedMessage {
	/// Message channel
	pub channel_id: ChannelId,
	/// Unique nonce to prevent replaying messages
	#[codec(compact)]
	pub nonce: u64,
	/// Command to execute in the Gateway contract
	pub command: u8,
	/// Params for the command
	pub params: Vec<u8>,
	/// Maximum gas allowed for message dispatch
	#[codec(compact)]
	pub max_dispatch_gas: u64,
	/// Maximum fee per gas
	#[codec(compact)]
	pub max_fee_per_gas: u128,
	/// Reward in ether for delivering this message, in addition to the gas refund
	#[codec(compact)]
	pub reward: u128,
	/// Message ID (Used for tracing messages across route, has no role in consensus)
	pub id: H256,
}

/// Convert message into an ABI-encoded form for delivery to the InboundQueue contract on Ethereum
impl From<CommittedMessage> for Token {
	fn from(x: CommittedMessage) -> Token {
		Token::Tuple(vec![
			Token::FixedBytes(Vec::from(x.channel_id.as_ref())),
			Token::Uint(x.nonce.into()),
			Token::Uint(x.command.into()),
			Token::Bytes(x.params.to_vec()),
			Token::Uint(x.max_dispatch_gas.into()),
			Token::Uint(x.max_fee_per_gas.into()),
			Token::Uint(x.reward.into()),
			Token::FixedBytes(Vec::from(x.id.as_ref())),
		])
	}
}

/// Configuration for fee calculations
#[derive(
	Encode,
	Decode,
	Copy,
	Clone,
	PartialEq,
	RuntimeDebug,
	MaxEncodedLen,
	TypeInfo,
	Serialize,
	Deserialize,
)]
pub struct FeeConfigRecord {
	/// ETH/DOT exchange rate
	pub exchange_rate: FixedU128,
	/// Ether fee per unit of gas
	pub fee_per_gas: u128,
	/// Ether reward for delivering message
	pub reward: u128,
}

#[derive(RuntimeDebug)]
pub struct InvalidFeeConfig;

impl FeeConfigRecord {
	pub fn validate(&self) -> Result<(), InvalidFeeConfig> {
		if self.exchange_rate == FixedU128::zero() {
			return Err(InvalidFeeConfig)
		}
		if self.fee_per_gas == 0 {
			return Err(InvalidFeeConfig)
		}
		if self.reward == 0 {
			return Err(InvalidFeeConfig)
		}
		Ok(())
	}
}
