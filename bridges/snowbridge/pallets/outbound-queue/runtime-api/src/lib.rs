// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
#![cfg_attr(not(feature = "std"), no_std)]

use frame_support::traits::tokens::Balance as BalanceT;
use snowbridge_core::{
	outbound::{Command, Fee},
	PricingParameters,
};
use snowbridge_outbound_queue_merkle_tree::MerkleProof;

sp_api::decl_runtime_apis! {
	pub trait OutboundQueueApi<Balance> where Balance: BalanceT
	{
		/// Generate a merkle proof for a committed message identified by `leaf_index`.
		/// The merkle root is stored in the block header as a
		/// `sp_runtime::generic::DigestItem::Other`
		fn prove_message(leaf_index: u64) -> Option<MerkleProof>;

		/// Calculate the delivery fee for `command`
		fn calculate_fee(command: Command, parameters: Option<PricingParameters<Balance>>) -> Fee<Balance>;
	}
}
