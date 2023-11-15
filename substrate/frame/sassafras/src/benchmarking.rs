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
use sp_consensus_sassafras::{vrf::VrfSignature, EpochConfiguration};
use sp_std::vec;

use frame_benchmarking::v2::*;
use frame_support::traits::Hooks;
use frame_system::RawOrigin;

const LOG_TARGET: &str = "sassafras::benchmark";

const TICKETS_DATA: &[u8] = include_bytes!("data/25_tickets_100_auths.bin");

fn make_dummy_vrf_signature() -> VrfSignature {
	// This leverages our knowledge about serialized vrf signature structure.
	// Mostly to avoid to import all the bandersnatch primitive just for this test.
	let buf = [
		0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
		0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
		0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
		0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
		0x00, 0x00, 0x00, 0x00, 0x00, 0x04, 0xb5, 0x5f, 0x8e, 0xc7, 0x68, 0xf5, 0x05, 0x3f, 0xa9,
		0x18, 0xca, 0x07, 0x13, 0xc7, 0x4b, 0xa3, 0x9a, 0x97, 0xd3, 0x76, 0x8f, 0x0c, 0xbf, 0x2e,
		0xd4, 0xf9, 0x3a, 0xae, 0xc1, 0x96, 0x2a, 0x64, 0x80,
	];
	VrfSignature::decode(&mut &buf[..]).unwrap()
}

#[benchmarks]
mod benchmarks {
	use super::*;

	// For first block (#1) we do some extra operation.
	// But is a one shot operation, so we don't account for it here.
	// We use 0, as it will be the path used by all the blocks with n != 1
	#[benchmark]
	fn on_initialize() {
		let block_num = BlockNumberFor::<T>::from(0u32);

		let slot_claim = SlotClaim {
			authority_idx: 0,
			slot: Default::default(),
			vrf_signature: make_dummy_vrf_signature(),
			ticket_claim: None,
		};
		frame_system::Pallet::<T>::deposit_log((&slot_claim).into());

		#[block]
		{
			// According to `Hooks` docs, `on_finalize` Weight should be bundled together
			// with `on_initialize`.
			Pallet::<T>::on_initialize(block_num);
			Pallet::<T>::on_finalize(block_num)
		}
	}

	// Weight for the default internal epoch change trigger.
	//
	// This accounts for the worst case where we need to recompute the ring verifier.
	//
	// The weight also slightly depends on the number of authorities in the next epoch.
	#[benchmark]
	fn internal_epoch_change_trigger(x: Linear<1, 100>) {
		let authorities_count = x as usize;

		let mut raw_data = TICKETS_DATA;
		let (authorities, _): (Vec<AuthorityId>, Vec<TicketEnvelope>) =
			Decode::decode(&mut raw_data).expect("Failed to decode tickets buffer");
		let next_authorities: Vec<_> = authorities[..authorities_count].to_vec();
		let next_authorities = WeakBoundedVec::force_from(next_authorities, None);
		NextAuthorities::<T>::set(next_authorities);

		#[block]
		{
			Pallet::<T>::should_end_epoch(BlockNumberFor::<T>::from(3u32));
			let next_authorities = Pallet::<T>::next_authorities();
			Pallet::<T>::enact_epoch_change(Default::default(), next_authorities);
		}
	}

	#[benchmark]
	fn submit_tickets(x: Linear<1, 25>) {
		let tickets_count = x as usize;

		let mut raw_data = TICKETS_DATA;
		let (authorities, tickets): (Vec<AuthorityId>, Vec<TicketEnvelope>) =
			Decode::decode(&mut raw_data).expect("Failed to decode tickets buffer");

		log::debug!(target: LOG_TARGET, "PreBuiltTickets: {} tickets, {} authorities", tickets.len(), authorities.len());

		Pallet::<T>::update_ring_verifier(&authorities);

		// Set next epoch config to accept all the tickets
		let next_config = EpochConfiguration { attempts_number: 1, redundancy_factor: u32::MAX };
		NextEpochConfig::<T>::set(Some(next_config));

		// Use the authorities in the pre-build tickets
		let authorities = WeakBoundedVec::force_from(authorities, None);
		NextAuthorities::<T>::set(authorities);

		let tickets = tickets[..tickets_count].to_vec();
		let tickets = BoundedVec::truncate_from(tickets);

		log::debug!(target: LOG_TARGET, "Submitting {} tickets", tickets_count);

		#[extrinsic_call]
		submit_tickets(RawOrigin::None, tickets);
	}

	#[benchmark]
	fn plan_config_change() {
		let config = EpochConfiguration { redundancy_factor: 1, attempts_number: 10 };

		#[extrinsic_call]
		plan_config_change(RawOrigin::Root, config);
	}

	// Construction of ring verifier
	#[benchmark]
	fn update_ring_verifier(x: Linear<1, 100>) {
		let authorities_count = x as usize;

		let mut raw_data = TICKETS_DATA;
		let (authorities, _): (Vec<AuthorityId>, Vec<TicketEnvelope>) =
			Decode::decode(&mut raw_data).expect("Failed to decode tickets buffer");
		let authorities: Vec<_> = authorities[..authorities_count].to_vec();

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
			let _ring_ctx = RingContext::<T>::get().unwrap();
		}
	}

	// Tickets segments sorting function benchmark.
	#[benchmark]
	fn sort_segments(x: Linear<1, 100>) {
		use sp_consensus_sassafras::EphemeralPublic;
		let segments_count = x as u32;
		let tickets_count = segments_count * SEGMENT_MAX_SIZE;

		// Construct a bunch of dummy tickets
		let tickets: Vec<_> = (0..tickets_count)
			.map(|i| {
				let body = TicketBody {
					attempt_idx: i,
					erased_public: EphemeralPublic([i as u8; 32]),
					revealed_public: EphemeralPublic([i as u8; 32]),
				};
				let id_bytes = crate::hashing::blake2_128(&i.to_le_bytes());
				let id = TicketId::from_le_bytes(id_bytes);
				(id, body)
			})
			.collect();

		for (chunk_id, chunk) in tickets.chunks(SEGMENT_MAX_SIZE as usize).enumerate() {
			let segment: Vec<TicketId> = chunk
				.iter()
				.map(|(id, body)| {
					TicketsData::<T>::set(id, Some(body.clone()));
					*id
				})
				.collect();
			let segment = BoundedVec::truncate_from(segment);
			UnsortedSegments::<T>::insert(chunk_id as u32, segment);
		}

		// Update metadata
		let mut meta = TicketsMeta::<T>::get();
		meta.unsorted_tickets_count = tickets_count;
		TicketsMeta::<T>::set(meta.clone());

		log::debug!(target: LOG_TARGET, "Before sort: {:?}", meta);
		#[block]
		{
			Pallet::<T>::sort_tickets(u32::MAX, 0, &mut meta);
		}
		log::debug!(target: LOG_TARGET, "After sort: {:?}", meta);
	}
}
