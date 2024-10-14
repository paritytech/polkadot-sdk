// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
use codec::{Decode, Encode};
use ethabi::Token;
use frame_support::{pallet_prelude::ConstU32, traits::ProcessMessage, BoundedVec};
use scale_info::TypeInfo;
use sp_core::H256;
use sp_runtime::RuntimeDebug;
use sp_std::prelude::*;

use super::Pallet;

pub use snowbridge_core::merkle::MerkleProof;
use snowbridge_core::outbound_v2::CommandWrapper;

pub type ProcessMessageOriginOf<T> = <Pallet<T> as ProcessMessage>::Origin;

pub const LOG_TARGET: &str = "snowbridge-outbound-queue";

/// Message which has been assigned a nonce and will be committed at the end of a block
#[derive(Encode, Decode, Clone, RuntimeDebug, TypeInfo)]
#[cfg_attr(feature = "std", derive(PartialEq))]
pub struct CommittedMessage {
	/// Origin of the message
	pub origin: H256,
	/// Unique nonce to prevent replaying messages
	pub nonce: u64,
	/// MessageId
	pub id: H256,
	/// Commands to execute in Ethereum
	pub commands: BoundedVec<CommandWrapper, ConstU32<5>>,
}

/// Convert message into an ABI-encoded form for delivery to the Gateway contract on Ethereum
impl From<CommittedMessage> for Token {
	fn from(x: CommittedMessage) -> Token {
		let header = vec![
			Token::FixedBytes(x.origin.as_bytes().to_owned()),
			Token::Uint(x.nonce.into()),
			Token::Uint(x.commands.len().into()),
		];
		let body: Vec<Token> = x.commands.into_iter().map(|command| command.into()).collect();
		let message = header.into_iter().chain(body.into_iter()).collect();
		Token::Tuple(message)
	}
}
