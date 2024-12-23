// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

//! Benchmarking for slots pallet

#[cfg(feature = "runtime-benchmarks")]
use super::*;
use frame_support::assert_ok;
use frame_system::RawOrigin;
use polkadot_runtime_parachains::paras;
use sp_runtime::traits::{Bounded, One};

use frame_benchmarking::{account, benchmarks, whitelisted_caller, BenchmarkError};

use crate::slots::Pallet as Slots;

fn assert_last_event<T: Config>(generic_event: <T as Config>::RuntimeEvent) {
	let events = frame_system::Pallet::<T>::events();
	let system_event: <T as frame_system::Config>::RuntimeEvent = generic_event.into();
	// compare to the last event record
	let frame_system::EventRecord { event, .. } = &events[events.len() - 1];
	assert_eq!(event, &system_event);
}

// Registers a parathread (on-demand parachain)
fn register_a_parathread<T: Config + paras::Config>(i: u32) -> (ParaId, T::AccountId) {
	let para = ParaId::from(i);
	let leaser: T::AccountId = account("leaser", i, 0);
	T::Currency::make_free_balance_be(&leaser, BalanceOf::<T>::max_value());
	let worst_head_data = T::Registrar::worst_head_data();
	let worst_validation_code = T::Registrar::worst_validation_code();

	assert_ok!(T::Registrar::register(
		leaser.clone(),
		para,
		worst_head_data,
		worst_validation_code.clone(),
	));
	assert_ok!(paras::Pallet::<T>::add_trusted_validation_code(
		frame_system::Origin::<T>::Root.into(),
		worst_validation_code,
	));

	T::Registrar::execute_pending_transitions();

	(para, leaser)
}

benchmarks! {
	where_clause { where T: paras::Config }

	force_lease {
		// If there is an offset, we need to be on that block to be able to do lease things.
		frame_system::Pallet::<T>::set_block_number(T::LeaseOffset::get() + One::one());
		let para = ParaId::from(1337);
		let leaser: T::AccountId = account("leaser", 0, 0);
		T::Currency::make_free_balance_be(&leaser, BalanceOf::<T>::max_value());
		let amount = T::Currency::minimum_balance();
		let period_begin = 69u32.into();
		let period_count = 3u32.into();
		let origin =
			T::ForceOrigin::try_successful_origin().map_err(|_| BenchmarkError::Weightless)?;
	}: _<T::RuntimeOrigin>(origin, para, leaser.clone(), amount, period_begin, period_count)
	verify {
		assert_last_event::<T>(Event::<T>::Leased {
			para_id: para,
			leaser, period_begin,
			period_count,
			extra_reserved: amount,
			total_amount: amount,
		}.into());
	}

	// Worst case scenario, T on-demand parachains onboard, and C lease holding parachains offboard.
	manage_lease_period_start {
		// Assume reasonable maximum of 100 paras at any time
		let c in 0 .. 100;
		let t in 0 .. 100;

		let period_begin = 1u32.into();
		let period_count = 4u32.into();

		// If there is an offset, we need to be on that block to be able to do lease things.
		frame_system::Pallet::<T>::set_block_number(T::LeaseOffset::get() + One::one());

		// Make T parathreads (on-demand parachains)
		let paras_info = (0..t).map(|i| {
			register_a_parathread::<T>(i)
		}).collect::<Vec<_>>();

		T::Registrar::execute_pending_transitions();

		// T on-demand parachains are upgrading to lease holding parachains
		for (para, leaser) in paras_info {
			let amount = T::Currency::minimum_balance();
			let origin = T::ForceOrigin::try_successful_origin()
				.expect("ForceOrigin has no successful origin required for the benchmark");
			Slots::<T>::force_lease(origin, para, leaser, amount, period_begin, period_count)?;
		}

		T::Registrar::execute_pending_transitions();

		// C lease holding parachains are downgrading to on-demand parachains
		for i in 200 .. 200 + c {
			let (para, leaser) = register_a_parathread::<T>(i);
			T::Registrar::make_parachain(para)?;
		}

		T::Registrar::execute_pending_transitions();

		for i in 0 .. t {
			assert!(T::Registrar::is_parathread(ParaId::from(i)));
		}

		for i in 200 .. 200 + c {
			assert!(T::Registrar::is_parachain(ParaId::from(i)));
		}
	}: {
			Slots::<T>::manage_lease_period_start(period_begin);
	} verify {
		// All paras should have switched.
		T::Registrar::execute_pending_transitions();
		for i in 0 .. t {
			assert!(T::Registrar::is_parachain(ParaId::from(i)));
		}
		for i in 200 .. 200 + c {
			assert!(T::Registrar::is_parathread(ParaId::from(i)));
		}
	}

	// Assume that at most 8 people have deposits for leases on a parachain.
	// This would cover at least 4 years of leases in the worst case scenario.
	clear_all_leases {
		let max_people = 8;
		let (para, _) = register_a_parathread::<T>(1);

		// If there is an offset, we need to be on that block to be able to do lease things.
		frame_system::Pallet::<T>::set_block_number(T::LeaseOffset::get() + One::one());

		for i in 0 .. max_people {
			let leaser = account("lease_deposit", i, 0);
			let amount = T::Currency::minimum_balance();
			T::Currency::make_free_balance_be(&leaser, BalanceOf::<T>::max_value());

			// Average slot has 4 lease periods.
			let period_count: LeasePeriodOf<T> = 4u32.into();
			let period_begin = period_count * i.into();
			let origin = T::ForceOrigin::try_successful_origin()
				.expect("ForceOrigin has no successful origin required for the benchmark");
			Slots::<T>::force_lease(origin, para, leaser, amount, period_begin, period_count)?;
		}

		for i in 0 .. max_people {
			let leaser = account("lease_deposit", i, 0);
			assert_eq!(T::Currency::reserved_balance(&leaser), T::Currency::minimum_balance());
		}

		let origin =
			T::ForceOrigin::try_successful_origin().map_err(|_| BenchmarkError::Weightless)?;
	}: _<T::RuntimeOrigin>(origin, para)
	verify {
		for i in 0 .. max_people {
			let leaser = account("lease_deposit", i, 0);
			assert_eq!(T::Currency::reserved_balance(&leaser), 0u32.into());
		}
	}

	trigger_onboard {
		// get a parachain into a bad state where they did not onboard
		let (para, _) = register_a_parathread::<T>(1);
		Leases::<T>::insert(para, vec![Some((account::<T::AccountId>("lease_insert", 0, 0), BalanceOf::<T>::default()))]);
		assert!(T::Registrar::is_parathread(para));
		let caller = whitelisted_caller();
	}: _(RawOrigin::Signed(caller), para)
	verify {
		T::Registrar::execute_pending_transitions();
		assert!(T::Registrar::is_parachain(para));
	}

	impl_benchmark_test_suite!(
		Slots,
		crate::integration_tests::new_test_ext(),
		crate::integration_tests::Test,
	);
}
