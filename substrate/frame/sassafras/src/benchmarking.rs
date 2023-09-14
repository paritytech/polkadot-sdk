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

use super::*;
use sp_consensus_sassafras::EpochConfiguration;
use sp_std::vec;

use frame_benchmarking::v2::{ParamRange, *};
use frame_system::RawOrigin;

const TICKETS_DATA: &[u8] = include_bytes!("../tickets.bin");

#[derive(Encode, Decode)]
struct PreBuiltTickets {
	authorities: Vec<AuthorityId>,
	tickets: Vec<TicketEnvelope>,
}

#[benchmarks]
mod benchmarks {
	use super::*;

	const LOG_TARGET: &str = "sassafras::benchmark";

	#[benchmark]
	fn submit_tickets(x: Linear<0, 3>) {
		let tickets_count = x as usize;

		let mut raw_data = TICKETS_DATA;
		let PreBuiltTickets { authorities, tickets } =
			PreBuiltTickets::decode(&mut raw_data).expect("Failed to decode tickets buffer");

		log::debug!(target: LOG_TARGET, "PreBuiltTickets: {} tickets, {} authorities", tickets.len(), authorities.len());

		let authorities = WeakBoundedVec::force_from(authorities, None);
		Authorities::<T>::set(authorities);

		let tickets = tickets[..tickets_count].to_vec();
		let tickets = BoundedVec::truncate_from(tickets);

		let ring_ctx = vrf::RingContext::new_testing();
		RingContext::<T>::set(Some(ring_ctx));

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

	#[benchmark]
	fn recompute_ring_verifier(x: Linear<1, 20>) {
		let authorities_count = x as usize;

		let ring_ctx = vrf::RingContext::new_testing();
		RingContext::<T>::set(Some(ring_ctx));

		let mut raw_data = TICKETS_DATA;
		let PreBuiltTickets { authorities, tickets: _ } =
			PreBuiltTickets::decode(&mut raw_data).expect("Failed to decode tickets buffer");
		let authorities: Vec<_> = authorities[..authorities_count].to_vec();

		#[block]
		{
			let ring_ctx = RingContext::<T>::get().unwrap();

			let pks: Vec<_> = authorities.iter().map(|auth| *auth.as_ref()).collect();
			let verifier = ring_ctx.verifier(&pks[..]);
		}
	}

	#[benchmark]
	fn recompute_ring_verifier_in_memory(x: Linear<1, 20>) {
		let authorities_count = x as usize;

		let ring_ctx = vrf::RingContext::new_testing();
		RingContext::<T>::set(Some(ring_ctx.clone()));

		let mut raw_data = TICKETS_DATA;
		let PreBuiltTickets { authorities, tickets: _ } =
			PreBuiltTickets::decode(&mut raw_data).expect("Failed to decode tickets buffer");
		let authorities: Vec<_> = authorities[..authorities_count].to_vec();

		let mut buf: Vec<u8> = ring_ctx.encode();

		#[block]
		{
			let ring_ctx = vrf::RingContext::decode(&mut buf.as_slice()).unwrap();

			let pks: Vec<_> = authorities.iter().map(|auth| *auth.as_ref()).collect();
			let verifier = ring_ctx.verifier(&pks[..]);
		}
	}

	// Internal function benchmarks
	#[benchmark]
	fn sort_segments(x: Linear<1, 1800>, y: Linear<1, 2>) {
		use sp_consensus_sassafras::EphemeralPublic;

		let tickets_count = <T as Config>::EpochDuration::get() as u32;
		let segments_count = x as u32;
		let max_segments = y as u32;

		let segment_len = 1 + (tickets_count - 1) / segments_count;

		log::debug!(target: LOG_TARGET, "------ segments: {}, max_segments: {}", segments_count, max_segments);

		// Construct a bunch of dummy tickets
		let tickets: Vec<_> = (0..tickets_count)
			.map(|i| {
				let body = TicketBody {
					attempt_idx: i,
					erased_public: EphemeralPublic([i as u8; 32]),
					revealed_public: EphemeralPublic([i as u8; 32]),
				};
				(i as TicketId, body)
			})
			.collect();

		for (chunk_id, chunk) in tickets.chunks(segment_len as usize).enumerate() {
			let segment: Vec<TicketId> = chunk
				.iter()
				.map(|(id, body)| {
					TicketsData::<T>::set(id, Some(body.clone()));
					*id
				})
				.collect();
			let segment = BoundedVec::truncate_from(segment);
			NextTicketsSegments::<T>::insert(chunk_id as u32, segment);
		}

		// Update metadata
		let mut meta = TicketsMeta::<T>::get();
		meta.segments_count += segments_count as u32;
		TicketsMeta::<T>::set(meta.clone());

		log::debug!(target: LOG_TARGET, "Before sort: {:?}", meta);

		#[block]
		{
			Pallet::<T>::sort_tickets(max_segments, 0, &mut meta);
		}

		log::debug!(target: LOG_TARGET, "After sort: {:?}", meta);
	}
}
