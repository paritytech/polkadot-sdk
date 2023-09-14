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

		// #[block]
		// {
		// 	let _authorities_num = T::MaxAuthorities::get();
		// 	// 	let authorities = Authorities::<T>::get();
		// 	// 	log::debug!(target: "sassafras", "AUTH NUM: {}", authorities.len());
		// }
	}

	#[benchmark]
	fn plan_config_change() {
		// Use some valid values
		let config = EpochConfiguration { redundancy_factor: 1, attempts_number: 10 };

		#[extrinsic_call]
		plan_config_change(RawOrigin::Root, config);
	}
}
