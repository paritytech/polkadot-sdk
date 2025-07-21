// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Various predefined implementations supporting header synchronization between BridgeHub
//! and other chains.

use alloc::vec::Vec;
use bp_parachains::{OnNewHead, ParaHash, ParaHead, ParaId};
use bp_runtime::{HeaderOf, Parachain};
use codec::Decode;
use frame_support::sp_runtime::traits::Header;
use frame_support::{traits::Get, weights::Weight};
use sp_runtime::traits::Convert;
use xcm::latest::prelude::*;

/// An adapter that implements `OnNewHead` to monitor parachain head updates (`ParaId/ParaHead`)
/// for scheduling syncs, and `OnSend` to send them to a preconfigured destination as an unpaid
/// `Transact` instruction.
pub struct SyncParaHeadersFor<T, I, Chain, Sender, Message>(
	core::marker::PhantomData<(T, I, Chain, Sender, Message)>,
);
impl<
		T: pallet_bridge_proof_root_sync::Config<I, Key = ParaId, Value = ParaHead>,
		I: 'static,
		Chain: Parachain<Hash = ParaHash>,
		Sender,
		Message,
	> OnNewHead for SyncParaHeadersFor<T, I, Chain, Sender, Message>
{
	fn on_new_head(id: ParaId, head: &ParaHead) -> Weight {
		// Filter by para ID.
		if Chain::PARACHAIN_ID != id.0 {
			return Weight::zero();
		}

		// Schedule for syncing.
		pallet_bridge_proof_root_sync::Pallet::<T, I>::schedule_for_sync(id, head.clone());

		// Schedule does one read and one write.
		T::DbWeight::get().reads_writes(1, 1)
	}
}

/// An `OnSend` implementation that sends `ParaId/ParaHead` as XCM.
impl<
		T: pallet_bridge_proof_root_sync::Config<I, Key = ParaId, Value = ParaHead>,
		I: 'static,
		Chain: Parachain<Hash = ParaHash>,
		XcmSender: SendXcm,
		MessageToDestination: Convert<Vec<(Chain::Hash, Chain::Hash)>, Xcm<()>> + Get<Location>,
	> pallet_bridge_proof_root_sync::OnSend<ParaId, ParaHead>
	for SyncParaHeadersFor<T, I, Chain, XcmSender, MessageToDestination>
{
	fn on_send(roots: &Vec<(ParaId, ParaHead)>) {
		const LOG_TARGET: &str = "runtime::proof-root-sync::on-send";

		// Extract just `(block_hash, state_root)` to minimalize message size.
		let roots = roots
			.iter()
			.filter_map(|(id, head)| {
				if Chain::PARACHAIN_ID != id.0 {
					return None;
				}

				// We just need block_hash and state_root.
				let header: HeaderOf<Chain> = match Decode::decode(&mut &head.0[..]) {
					Ok(header) => header,
					Err(error) => {
						tracing::warn!(
							target: LOG_TARGET,
							?head,
							para_id = ?id,
							?error,
							"Failed to decode parachain header - skipping it!",
						);
						return None;
					},
				};
				Some((header.hash(), *header.state_root()))
			})
			.collect::<Vec<_>>();

		// Send dedicated `Transact` to dest.
		let xcm = MessageToDestination::convert(roots);
		let dest = MessageToDestination::get();
		if let Err(error) = send_xcm::<XcmSender>(dest, xcm) {
			tracing::warn!(
				target: "runtime::bridge-xcm::on-send",
				?error,
				dest = ?MessageToDestination::get(),
				"Failed to send XCM"
			);
		}
	}
}

#[cfg(feature = "runtime-benchmarks")]
impl<
		T: pallet_bridge_proof_root_sync::Config<I, Key = ParaId, Value = ParaHead>,
		I: 'static,
		Chain: Parachain<Hash = ParaHash>,
		Sender,
		Message,
	> pallet_bridge_proof_root_sync::BenchmarkHelper<ParaId, ParaHead>
	for SyncParaHeadersFor<T, I, Chain, Sender, Message>
{
	fn create_key_value_for(id: u32) -> (ParaId, ParaHead) {
		use codec::Encode;

		let para_header_number = id;
		let mut para_hash = [0_u8; 32];
		para_hash[..4].copy_from_slice(&para_header_number.to_le_bytes());
		let para_state_root = ParaHash::from(para_hash);

		let para_head = ParaHead(
			bp_test_utils::test_header_with_root::<HeaderOf<Chain>>(
				para_header_number.into(),
				para_state_root,
			)
			.encode(),
		);

		(ParaId::from(Chain::PARACHAIN_ID), para_head)
	}
}
