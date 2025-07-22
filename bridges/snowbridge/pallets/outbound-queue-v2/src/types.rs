// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
use super::Pallet;
use codec::{Decode, Encode};
use frame_support::traits::ProcessMessage;
use scale_info::TypeInfo;
pub use snowbridge_merkle_tree::MerkleProof;
use snowbridge_outbound_queue_primitives::v2::OutboundMessage;
use sp_core::H256;
use sp_runtime::RuntimeDebug;
use sp_std::prelude::*;

pub type ProcessMessageOriginOf<T> = <Pallet<T> as ProcessMessage>::Origin;

/// Pending order
#[derive(Encode, Decode, TypeInfo, Clone, PartialEq, RuntimeDebug)]
pub struct PendingOrder<BlockNumber> {
	/// The nonce used to identify the message
	pub nonce: u64,
	/// The block number in which the message was processed
	pub block_number: BlockNumber,
	/// The fee in Ether provided by the user to incentivize message delivery
	#[codec(compact)]
	pub fee: u128,
	/// The hash of the message
	pub hash: H256,
	/// The original OutboundMessage
	pub outbound_message: OutboundMessage,
	/// The block number in which the message was committed
	pub committed_block_number: Option<BlockNumber>,
}
