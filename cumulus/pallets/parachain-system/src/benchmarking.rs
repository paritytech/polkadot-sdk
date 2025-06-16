// This file is part of Cumulus.

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

//! Benchmarking for the parachain-system pallet.

#![cfg(feature = "runtime-benchmarks")]

use super::*;
use cumulus_primitives_core::relay_chain::Hash as RelayHash;
use frame_benchmarking::v2::*;
use sp_runtime::traits::BlakeTwo256;
use frame_system::{RawOrigin, pallet_prelude::BlockNumberFor};
use frame_support::traits::{OnInitialize, OnFinalize};
use sp_trie::StorageProof;
use polkadot_parachain_primitives::primitives::HeadData;
use std::collections::BTreeSet;
use cumulus_primitives_core::relay_chain::DownwardMessage;

#[benchmarks]
mod benchmarks {
	use super::*;

	/// Enqueue `n` messages via `enqueue_inbound_downward_messages`.
	///
	/// The limit is set to `1000` for benchmarking purposes as the actual limit is only known at
	/// runtime. However, the limit (and default) for Dotsama are magnitudes smaller.
	#[benchmark]
	fn enqueue_inbound_downward_messages(n: Linear<0, 1000>) {
		let msg = InboundDownwardMessage {
			sent_at: n, // The block number does not matter.
			msg: vec![0u8; MaxDmpMessageLenOf::<T>::get() as usize],
		};
		let msgs = vec![msg; n as usize];
		let head = mqp_head(&msgs);

		#[block]
		{
			Pallet::<T>::enqueue_inbound_downward_messages(head, msgs);
		}

		assert_eq!(ProcessedDownwardMessages::<T>::get(), n);
		assert_eq!(LastDmqMqcHead::<T>::get().head(), head);
	}

	/// Re-implements an easy version of the `MessageQueueChain` for testing purposes.
	fn mqp_head(msgs: &Vec<InboundDownwardMessage>) -> RelayHash {
		let mut head = Default::default();
		for msg in msgs.iter() {
			let msg_hash = BlakeTwo256::hash_of(&msg.msg);
			head = BlakeTwo256::hash_of(&(head, msg.sent_at, msg_hash));
		}
		head
	}

	#[benchmark]
	fn set_validation_data(n: Linear<0, 1000>) { // use run to block to move up as we go
		Pallet::<T>::on_initialize(BlockNumberFor::<T>::one());

		// Initialize UsedBandwidth with dummy values
        let ump_msg_count = 2; // 2 UMP messages
        let ump_total_bytes = 300; // 300 bytes of UMP messages
        let mut hrmp_outgoing = BTreeMap::new();
        hrmp_outgoing.insert(
            ParaId::from(2000), // Recipient parachain ID
            HrmpChannelUpdate { msg_count: 1, total_bytes: 100 }, // 1 HRMP message, 100 bytes
        );
        hrmp_outgoing.insert(
            ParaId::from(2001), // Recipient parachain ID
            HrmpChannelUpdate { msg_count: 2, total_bytes: 200 }, // 2 HRMP messages, 200 bytes
        );

        let used_bandwidth = UsedBandwidth {
            ump_msg_count,
            ump_total_bytes,
            hrmp_outgoing,
        };

        // Initialize the unincluded segment with a dummy Ancestor
        let ancestor = Ancestor::new_unchecked(used_bandwidth, None);
        <UnincludedSegment<T>>::put(sp_std::vec![ancestor]);

		// Create a dummy StorageProof
        let trie_nodes = BTreeSet::from_iter(vec![
            vec![0u8; 32], // Dummy trie node
            vec![1u8; 32], // Dummy trie node
        ]);

		// Setup the persisted validation data
        let parent_head = HeadData(vec![0u8; 32]); // Dummy parent head data
        let relay_parent_number = 1u32.into(); // Dummy relay chain block number
        let relay_parent_storage_root = Default::default(); // Dummy relay chain storage root
        let max_pov_size = 1024; // Dummy max PoV size

        let validation_data = PersistedValidationData {
            parent_head,
            relay_parent_number,
            relay_parent_storage_root,
            max_pov_size,
        };

        let relay_chain_state = StorageProof::new(trie_nodes);

		// Setup downward messages
        let msg = InboundDownwardMessage {
			sent_at: n, // The block number does not matter.
			msg: vec![0u8; MaxDmpMessageLenOf::<T>::get() as usize],
		};
		let msgs = vec![msg; n as usize];

		let mut horizontal_messages = BTreeMap::new();
        horizontal_messages.insert(
            ParaId::from(1000), // Sender parachain ID
            vec![
                InboundHrmpMessage {
                    sent_at: 1u32.into(),
                    data: vec![0u8; 100],
                },
                InboundHrmpMessage {
                    sent_at: 2u32.into(),
                    data: vec![0u8; 200],
                },
            ],
        );

        // Create the ParachainInherentData
        let data = ParachainInherentData {
            validation_data,
            relay_chain_state,
            downward_messages: msgs,
            horizontal_messages,
        };

		// Create Dummy ParachainInherentData

		// Setup the validation data
		// let validation_data: PersistedValidationData = Default::default();

		#[block]
		{
			let _ = Pallet::<T>::set_validation_data(RawOrigin::None.into(), data);
		}
		Pallet::<T>::on_finalize(BlockNumberFor::<T>::one());
	}

	impl_benchmark_test_suite! {
		Pallet,
		crate::mock::new_test_ext(),
		crate::mock::Test
	}
}
