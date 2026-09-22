// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//  http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use pallet_revive_types::runtime_api::*;

use crate::evm::{Block, HashesOrTransactionInfos};

pub struct BlockInputPayload;

impl From<BlockVersionedInputPayload> for BlockInputPayload {
	fn from(value: BlockVersionedInputPayload) -> Self {
		match value {
			BlockVersionedInputPayload::V1(payload) => payload.into(),
			BlockVersionedInputPayload::V2(payload) => payload.into(),
		}
	}
}

impl From<BlockInputPayloadV1> for BlockInputPayload {
	fn from(_value: BlockInputPayloadV1) -> Self {
		Self
	}
}

impl From<BlockInputPayloadV2> for BlockInputPayload {
	fn from(_value: BlockInputPayloadV2) -> Self {
		Self
	}
}

pub struct BlockOutputPayload {
	pub block: Block,
	/// Whether the trailing entry of `block.transactions` is the block's synthetic transaction.
	pub has_synthetic_transaction: bool,
}

impl From<BlockOutputPayload> for BlockOutputPayloadV1 {
	/// Drops the synthetic transaction's hash: V1 consumers pair `transactions` with the V1
	/// receipt data, which has no entry for it, and would serve a transaction they cannot find a
	/// receipt for.
	fn from(value: BlockOutputPayload) -> Self {
		let mut block = value.block;
		if value.has_synthetic_transaction {
			if let HashesOrTransactionInfos::Hashes(hashes) = &mut block.transactions {
				hashes.pop();
			}
		}
		Self { block: block.into() }
	}
}

impl From<BlockOutputPayload> for BlockOutputPayloadV2 {
	fn from(value: BlockOutputPayload) -> Self {
		Self { block: value.block.into() }
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use sp_core::H256;

	fn output(has_synthetic_transaction: bool) -> BlockOutputPayload {
		let block = Block {
			transactions: HashesOrTransactionInfos::Hashes(vec![
				H256::from([0x11; 32]),
				H256::from([0x22; 32]),
			]),
			..Default::default()
		};
		BlockOutputPayload { block, has_synthetic_transaction }
	}

	fn hashes(block: BlockV1) -> Vec<H256> {
		match block.transactions {
			HashesOrTransactionInfosV1::Hashes(hashes) => hashes,
			_ => panic!("the runtime commits transaction hashes"),
		}
	}

	#[test]
	fn v1_drops_the_synthetic_transaction_hash() {
		// The synthetic transaction is the block's trailing one. A V1 consumer has no receipt for
		// it, so V1 lists one hash per ethereum transaction, as the V1 receipt data has one entry.
		let v1 = BlockOutputPayloadV1::from(output(true));

		assert_eq!(hashes(v1.block), vec![H256::from([0x11; 32])]);
	}

	#[test]
	fn v1_keeps_every_hash_of_a_block_without_a_synthetic_transaction() {
		let v1 = BlockOutputPayloadV1::from(output(false));

		assert_eq!(hashes(v1.block), vec![H256::from([0x11; 32]), H256::from([0x22; 32])]);
	}

	#[test]
	fn v2_lists_the_synthetic_transaction() {
		let v2 = BlockOutputPayloadV2::from(output(true));

		assert_eq!(hashes(v2.block), vec![H256::from([0x11; 32]), H256::from([0x22; 32])]);
	}
}
