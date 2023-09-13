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
use sp_std::vec;

// use sp_consensus_sassafras::EpochConfiguration;

use frame_benchmarking::v2::*;
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

	#[benchmark]
	fn check_ticket() {
		// let mut raw_data = TICKETS_DATA;
		// let PreBuiltTickets { authorities, tickets } =
		// 	PreBuiltTickets::decode(&mut raw_data).expect("Failed to decode tickets buffer");

		// let authorities = WeakBoundedVec::force_from(authorities, None);
		// let tickets = tickets[0..1].to_vec();
		// let tickets = BoundedVec::truncate_from(tickets);

		// Authorities::<T>::set(authorities);

		let ring_ctx = vrf::RingContext::new_testing();
		RingContext::<T>::set(Some(ring_ctx));

		// let caller: T::AccountId = whitelisted_caller();

		// #[extrinsic_call]
		// submit_tickets(RawOrigin::None, tickets);

		#[block]
		{
			let _authorities_num = T::MaxAuthorities::get();
			// 	let authorities = Authorities::<T>::get();
			// 	log::debug!(target: "sassafras", "AUTH NUM: {}", authorities.len());
		}
	}
}
// submit_tickets {
// 	let x in 0 .. <T as Config>::MaxTickets::get();

// 	// let tickets: BoundedVec<TicketEnvelope, <T as Config>::MaxTickets> =
// 	// 	(0..x).map(make_dummy_ticket).collect::<Vec<_>>().try_into().unwrap();
// }: _(RawOrigin::None, tickets)

// #[panic_handler]
// #[no_mangle]
// pub fn panic(info: &core::panic::PanicInfo) -> ! {
// 	let message = sp_std::alloc::format!("{}", info);
// 	#[cfg(feature = "improved_panic_error_reporting")]
// 	{
// 		panic_handler::abort_on_panic(&message);
// 	}
// 	#[cfg(not(feature = "improved_panic_error_reporting"))]
// 	{
// 		logging::log(LogLevel::Error, "runtime", message.as_bytes());
// 		core::arch::wasm32::unreachable();
// 	}
// }
