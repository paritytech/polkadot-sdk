// This file is part of Substrate.

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

//! Benchmarks for the Sassafras pallet.

use crate::*;
use sp_consensus_sassafras::vrf::VrfSignature;

use frame_benchmarking::v2::*;
use frame_support::traits::Hooks;
use frame_system::RawOrigin;

const LOG_TARGET: &str = "sassafras::benchmark";

// Pre-constructed tickets generated via the `generate_test_teckets` function
const TICKETS_DATA: &[u8] = include_bytes!("data/tickets.bin");

fn dummy_vrf_signature() -> VrfSignature {
	// This leverages our knowledge about serialized vrf signature structure.
	// Mostly to avoid to import all the bandersnatch primitive just for this test.
	const RAW_VRF_SIGNATURE: [u8; 99] = [
		0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
		0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
		0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
		0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
		0x00, 0x00, 0x00, 0x00, 0x00, 0x04, 0xb5, 0x5f, 0x8e, 0xc7, 0x68, 0xf5, 0x05, 0x3f, 0xa9,
		0x18, 0xca, 0x07, 0x13, 0xc7, 0x4b, 0xa3, 0x9a, 0x97, 0xd3, 0x76, 0x8f, 0x0c, 0xbf, 0x2e,
		0xd4, 0xf9, 0x3a, 0xae, 0xc1, 0x96, 0x2a, 0x64, 0x80,
	];
	VrfSignature::decode(&mut &RAW_VRF_SIGNATURE[..]).unwrap()
}

#[benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn on_initialize() {
		let block_num = BlockNumberFor::<T>::from(0u32);

		let slot_claim = SlotClaim {
			authority_idx: 0,
			slot: Default::default(),
			vrf_signature: dummy_vrf_signature(),
		};
		frame_system::Pallet::<T>::deposit_log((&slot_claim).into());

		#[block]
		{
			// According to `Hooks` trait docs, `on_finalize` `Weight` should be bundled
			// together with `on_initialize` `Weight`.
			Pallet::<T>::on_initialize(block_num);
			Pallet::<T>::on_finalize(block_num)
		}
	}

	// Weight for the default internal epoch change trigger.
	//
	// Parameters:
	// - `x`: number of authorities [1:100].
	// - `y`: number of tickets [100:1000];
	//
	// This accounts for the worst case which includes:
	// - recomputing the ring verifier key from a new authorites set.
	// - picking all the tickets from the accumulator in one shot.
	#[benchmark]
	fn enact_epoch_change(x: Linear<1, 100>, y: Linear<100, 1000>) {
		let authorities_count = x as usize;
		let accumulated_tickets = y as u32;

		let config = Pallet::<T>::protocol_config();

		// Makes the epoch change legit
		let post_init_cache = EphemeralData {
			prev_slot: Slot::from(config.epoch_length as u64 - 1),
			block_randomness: Randomness::default(),
		};
		TemporaryData::<T>::put(post_init_cache);
		CurrentSlot::<T>::set(Slot::from(config.epoch_length as u64));

		// Force ring verifier key re-computation
		let next_authorities: Vec<_> =
			Authorities::<T>::get().into_iter().cycle().take(authorities_count).collect();
		let next_authorities = WeakBoundedVec::force_from(next_authorities, None);
		NextAuthorities::<T>::set(next_authorities);

		// Add tickets to the accumulator
		(0..accumulated_tickets).for_each(|i| {
			let mut id = TicketId([0xff; 32]);
			id.0[..4].copy_from_slice(&i.to_be_bytes()[..]);
			let body = TicketBody { id, attempt: 0, extra: Default::default() };
			TicketsAccumulator::<T>::insert(TicketKey::from(id), &body);
		});

		#[block]
		{
			// Also account for the call typically done in case of epoch change
			Pallet::<T>::should_end_epoch(BlockNumberFor::<T>::from(3u32));
			let next_authorities = Pallet::<T>::next_authorities();
			// Using a different set of authorities triggers the recomputation of ring verifier.
			Pallet::<T>::enact_epoch_change(Default::default(), next_authorities);
		}
	}

	#[benchmark]
	fn submit_tickets(x: Linear<1, 16>) {
		let tickets_count = x as usize;

		let mut raw_data = TICKETS_DATA;
		let (randomness, authorities, tickets): (
			Randomness,
			Vec<AuthorityId>,
			Vec<TicketEnvelope>,
		) = Decode::decode(&mut raw_data).expect("Failed to decode tickets buffer");
		assert!(tickets.len() >= tickets_count);

		// Use the same values used for the pre-built tickets
		Pallet::<T>::update_ring_verifier(&authorities);
		NextAuthorities::<T>::set(WeakBoundedVec::force_from(authorities, None));
		let mut randomness_buf = RandomnessBuf::<T>::get();
		randomness_buf[2] = randomness;
		RandomnessBuf::<T>::set(randomness_buf);

		let tickets = tickets[..tickets_count].to_vec();
		let tickets = BoundedVec::truncate_from(tickets);

		#[extrinsic_call]
		submit_tickets(RawOrigin::None, tickets);
	}

	// Construction of ring verifier
	#[benchmark]
	fn update_ring_verifier(x: Linear<1, 100>) {
		let authorities_count = x as usize;
		let authorities: Vec<_> =
			Authorities::<T>::get().into_iter().cycle().take(authorities_count).collect();

		#[block]
		{
			Pallet::<T>::update_ring_verifier(&authorities);
		}
	}

	// Bare loading of ring context.
	//
	// It is interesting to see how this compares to 'update_ring_verifier', which
	// also recomputes and stores the new verifier.
	#[benchmark]
	fn load_ring_context() {
		#[block]
		{
			let _ = RingContext::<T>::get().unwrap();
		}
	}
}
