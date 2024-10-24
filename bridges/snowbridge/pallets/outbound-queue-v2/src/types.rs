// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
use super::Pallet;
use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::traits::ProcessMessage;
use scale_info::TypeInfo;
pub use snowbridge_merkle_tree::MerkleProof;
use sp_runtime::RuntimeDebug;
use sp_std::prelude::*;

pub type ProcessMessageOriginOf<T> = <Pallet<T> as ProcessMessage>::Origin;

/// Fee with block number for easy fetch the pending message on relayer side
#[derive(Encode, Decode, TypeInfo, Clone, Eq, PartialEq, RuntimeDebug, MaxEncodedLen)]
pub struct FeeWithBlockNumber<BlockNumber> {
	/// A nonce of the message for enforcing replay protection
	pub nonce: u64,
	/// The block number in which the message was committed
	pub block_number: BlockNumber,
	/// The fee
	pub fee: u128,
}
