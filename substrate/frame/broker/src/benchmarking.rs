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

#![cfg(feature = "runtime-benchmarks")]

use super::*;

use crate::{CoreAssignment::Task, Pallet as Broker};
use alloc::{vec, vec::Vec};
use frame_benchmarking::v2::*;
use frame_support::{
	storage::bounded_vec::BoundedVec,
	traits::{
		fungible::{Inspect, Mutate},
		EnsureOrigin, Hooks,
	},
};
use frame_system::{Pallet as System, RawOrigin};
use sp_core::Get;
use sp_runtime::{
	traits::{BlockNumberProvider, MaybeConvert},
	DispatchError, Saturating,
};

const SEED: u32 = 0;
const MAX_CORE_COUNT: u16 = 1_000;

const REGION_PRICE: u32 = 10;
const REGION_RENEWAL_PRICE: u32 = 20;
const REGION_LENGTH: Timeslice = 3;

fn assert_last_event<T: Config>(generic_event: <T as Config>::RuntimeEvent) {
	frame_system::Pallet::<T>::assert_last_event(generic_event.into());
}

fn assert_has_event<T: Config>(generic_event: <T as Config>::RuntimeEvent) {
	frame_system::Pallet::<T>::assert_has_event(generic_event.into());
}

fn get_pallet_events<T: Config>() -> Vec<Event<T>> {
	frame_system::Pallet::<T>::events()
		.into_iter()
		.rev()
		.filter_map(|event| (<T as Config>::RuntimeEvent::from(event.event)).try_into().ok())
		.collect()
}

fn new_config_record<T: Config>() -> ConfigRecordOf<T> {
	ConfigRecord {
		advance_notice: 2u32.into(),
		region_length: REGION_LENGTH,
		contribution_timeout: 5,
	}
}

fn new_schedule() -> Schedule {
	// Max items for worst case
	let mut items = Vec::new();
	for i in 0..CORE_MASK_BITS {
		items.push(ScheduleItem {
			assignment: Task(i.try_into().unwrap()),
			mask: CoreMask::complete(),
		});
	}
	Schedule::truncate_from(items)
}

fn setup_reservations<T: Config>(n: u32) {
	let schedule = new_schedule();

	Reservations::<T>::put(BoundedVec::try_from(vec![schedule.clone(); n as usize]).unwrap());
}

fn setup_leases<T: Config>(n: u32, task: u32, until: u32) {
	Leases::<T>::put(
		BoundedVec::try_from(vec![LeaseRecordItem { task, until: until.into() }; n as usize])
			.unwrap(),
	);
}

fn advance_to<T: Config>(b: u32) {
	while System::<T>::block_number() < b.into() {
		System::<T>::set_block_number(System::<T>::block_number().saturating_add(1u32.into()));

		let block_number: u32 = System::<T>::block_number().try_into().ok().unwrap();

		RCBlockNumberProviderOf::<T::Coretime>::set_block_number(block_number.into());
		Broker::<T>::on_initialize(System::<T>::block_number());
	}
}

struct StartedSale {
	first_core: CoreIndex,
}

fn setup_and_start_sale<T: Config>() -> Result<StartedSale, BenchmarkError> {
	Configuration::<T>::put(new_config_record::<T>());

	// Assume Reservations to be filled for worst case
	setup_reservations::<T>(T::MaxReservedCores::get());

	// Assume Leases to be filled for worst case
	setup_leases::<T>(T::MaxLeasedCores::get(), 1, 10);

	Broker::<T>::do_start_sales(Default::default(), MAX_CORE_COUNT.into())
		.map_err(|_| BenchmarkError::Weightless)?;

	let sale_data = StartedSale {
		first_core: T::MaxReservedCores::get()
			.saturating_add(T::MaxLeasedCores::get())
			.try_into()
			.unwrap(),
	};

	Ok(sale_data)
}

fn purchase_and_get_region_id<T: Config>(who: T::AccountId) -> Result<RegionId, DispatchError> {
	let price_limit = u32::MAX.into();
	Broker::<T>::do_purchase(who, price_limit)?;

	for event in get_pallet_events().into_iter().rev() {
		if let Event::<T>::Purchased { region_id, .. } = event {
			return Ok(region_id);
		}
	}

	panic!("If do_purchase succeeded the Purchased event must've been deposited")
}

#[benchmarks]
mod benches {
	use frame_support::weights::WeightMeter;

	use super::*;
	use crate::Finality::*;

	#[benchmark]
	fn configure() -> Result<(), BenchmarkError> {
		let config = new_config_record::<T>();

		let origin =
			T::AdminOrigin::try_successful_origin().map_err(|_| BenchmarkError::Weightless)?;

		#[extrinsic_call]
		_(origin as T::RuntimeOrigin, config.clone());

		assert_eq!(Configuration::<T>::get(), Some(config));

		Ok(())
	}

	#[benchmark]
	fn reserve() -> Result<(), BenchmarkError> {
		let schedule = new_schedule();

		// Assume Reservations to be almost filled for worst case
		setup_reservations::<T>(T::MaxReservedCores::get().saturating_sub(1));

		let origin =
			T::AdminOrigin::try_successful_origin().map_err(|_| BenchmarkError::Weightless)?;

		#[extrinsic_call]
		_(origin as T::RuntimeOrigin, schedule);

		assert_eq!(Reservations::<T>::get().len(), T::MaxReservedCores::get() as usize);

		Ok(())
	}

	#[benchmark]
	fn unreserve() -> Result<(), BenchmarkError> {
		// Assume Reservations to be filled for worst case
		setup_reservations::<T>(T::MaxReservedCores::get());

		let origin =
			T::AdminOrigin::try_successful_origin().map_err(|_| BenchmarkError::Weightless)?;

		#[extrinsic_call]
		_(origin as T::RuntimeOrigin, 0);

		assert_eq!(
			Reservations::<T>::get().len(),
			T::MaxReservedCores::get().saturating_sub(1) as usize
		);

		Ok(())
	}

	#[benchmark]
	fn set_lease() -> Result<(), BenchmarkError> {
		let task = 1u32;
		let until = 10u32.into();

		// Assume Leases to be almost filled for worst case
		setup_leases::<T>(T::MaxLeasedCores::get().saturating_sub(1), task, until);

		let origin =
			T::AdminOrigin::try_successful_origin().map_err(|_| BenchmarkError::Weightless)?;

		#[extrinsic_call]
		_(origin as T::RuntimeOrigin, task, until);

		assert_eq!(Leases::<T>::get().len(), T::MaxLeasedCores::get() as usize);

		Ok(())
	}

	#[benchmark]
	fn remove_lease() -> Result<(), BenchmarkError> {
		let task = 1u32;
		let until = 10u32;

		// Assume Leases to be almost filled for worst case
		let mut leases = vec![
			LeaseRecordItem { task, until };
			T::MaxLeasedCores::get().saturating_sub(1) as usize
		];
		let task = 2u32;
		leases.push(LeaseRecordItem { task, until });
		Leases::<T>::put(BoundedVec::try_from(leases).unwrap());

		let origin =
			T::AdminOrigin::try_successful_origin().map_err(|_| BenchmarkError::Weightless)?;

		#[extrinsic_call]
		_(origin as T::RuntimeOrigin, task);

		assert_eq!(Leases::<T>::get().len(), T::MaxLeasedCores::get().saturating_sub(1) as usize);

		Ok(())
	}

	#[benchmark]
	fn start_sales(n: Linear<0, { MAX_CORE_COUNT.into() }>) -> Result<(), BenchmarkError> {
		let config = new_config_record::<T>();
		Configuration::<T>::put(config.clone());

		let mut extra_cores = n;

		// Assume Reservations to be filled for worst case
		setup_reservations::<T>(extra_cores.min(T::MaxReservedCores::get()));
		extra_cores = extra_cores.saturating_sub(T::MaxReservedCores::get());

		// Assume Leases to be filled for worst case
		setup_leases::<T>(extra_cores.min(T::MaxLeasedCores::get()), 1, 10);
		extra_cores = extra_cores.saturating_sub(T::MaxLeasedCores::get());

		let latest_region_begin = Broker::<T>::latest_timeslice_ready_to_commit(&config);

		let origin =
			T::AdminOrigin::try_successful_origin().map_err(|_| BenchmarkError::Weightless)?;

		#[extrinsic_call]
		_(origin as T::RuntimeOrigin, Default::default(), extra_cores.try_into().unwrap());

		let sale_start = RCBlockNumberProviderOf::<T::Coretime>::current_block_number();
		assert_last_event::<T>(
			Event::SaleInitialized {
				sale_start,
				region_begin: latest_region_begin + config.region_length,
				region_end: latest_region_begin + config.region_length * 2,
				cores_offered: n
					.saturating_sub(T::MaxReservedCores::get())
					.saturating_sub(T::MaxLeasedCores::get())
					.try_into()
					.unwrap(),
			}
			.into(),
		);

		Ok(())
	}

	#[benchmark]
	fn purchase() -> Result<(), BenchmarkError> {
		let sale_data = setup_and_start_sale::<T>()?;

		advance_to::<T>(2);

		let caller: T::AccountId = whitelisted_caller();
		T::Currency::set_balance(
			&caller.clone(),
			T::Currency::minimum_balance().saturating_add(REGION_PRICE.into()),
		);

		#[extrinsic_call]
		_(RawOrigin::Signed(caller.clone()), REGION_PRICE.into());

		let region_begin =
			T::CoretimeMarket::get_region_begin().map_err(|_| BenchmarkError::Weightless)?;
		assert_last_event::<T>(
			Event::Purchased {
				who: caller,
				region_id: RegionId {
					begin: region_begin,
					core: sale_data.first_core,
					mask: CoreMask::complete(),
				},
				price: REGION_PRICE.into(),
				duration: 3u32.into(),
			}
			.into(),
		);

		Ok(())
	}

	#[benchmark]
	fn renew() -> Result<(), BenchmarkError> {
		setup_and_start_sale::<T>()?;
		let region_len = Configuration::<T>::get().unwrap().region_length;

		advance_to::<T>(2);

		let caller: T::AccountId = whitelisted_caller();
		T::Currency::set_balance(
			&caller.clone(),
			T::Currency::minimum_balance()
				.saturating_add(REGION_RENEWAL_PRICE.into())
				.saturating_add(REGION_PRICE.into()),
		);

		let region = purchase_and_get_region_id::<T>(caller.clone())
			.expect("Offer not high enough for configuration.");

		Broker::<T>::do_assign(region, None, 1001, Final)
			.map_err(|_| BenchmarkError::Weightless)?;

		advance_to::<T>((T::TimeslicePeriod::get() * region_len.into()).try_into().ok().unwrap());

		#[extrinsic_call]
		_(RawOrigin::Signed(caller), region.core);

		let id = PotentialRenewalId { core: region.core, when: region.begin + region_len * 2 };
		assert!(PotentialRenewals::<T>::get(id).is_some());

		Ok(())
	}

	#[benchmark]
	fn transfer() -> Result<(), BenchmarkError> {
		setup_and_start_sale::<T>()?;

		advance_to::<T>(2);

		let caller: T::AccountId = whitelisted_caller();
		T::Currency::set_balance(
			&caller.clone(),
			T::Currency::minimum_balance().saturating_add(REGION_PRICE.into()),
		);

		let region = purchase_and_get_region_id::<T>(caller.clone())
			.expect("Offer not high enough for configuration.");

		let recipient: T::AccountId = account("recipient", 0, SEED);

		#[extrinsic_call]
		_(RawOrigin::Signed(caller.clone()), region, recipient.clone());

		assert_last_event::<T>(
			Event::Transferred {
				region_id: region,
				old_owner: Some(caller),
				owner: Some(recipient),
				duration: 3u32.into(),
			}
			.into(),
		);

		Ok(())
	}

	#[benchmark]
	fn partition() -> Result<(), BenchmarkError> {
		let sale_data = setup_and_start_sale::<T>()?;

		advance_to::<T>(2);

		let caller: T::AccountId = whitelisted_caller();
		T::Currency::set_balance(
			&caller.clone(),
			T::Currency::minimum_balance().saturating_add(REGION_PRICE.into()),
		);

		let region = purchase_and_get_region_id::<T>(caller.clone())
			.expect("Offer not high enough for configuration.");

		// Worst case has an existing provisional pool assignment.
		Broker::<T>::do_pool(region, None, caller.clone(), Finality::Provisional)
			.map_err(|_| BenchmarkError::Weightless)?;

		#[extrinsic_call]
		_(RawOrigin::Signed(caller), region, 2);

		assert_last_event::<T>(
			Event::Partitioned {
				old_region_id: RegionId {
					begin: region.begin,
					core: sale_data.first_core,
					mask: CoreMask::complete(),
				},
				new_region_ids: (
					RegionId {
						begin: region.begin,
						core: sale_data.first_core,
						mask: CoreMask::complete(),
					},
					RegionId {
						begin: region.begin + 2,
						core: sale_data.first_core,
						mask: CoreMask::complete(),
					},
				),
			}
			.into(),
		);

		Ok(())
	}

	#[benchmark]
	fn interlace() -> Result<(), BenchmarkError> {
		let sale_data = setup_and_start_sale::<T>()?;
		let core = sale_data.first_core;

		advance_to::<T>(2);

		let caller: T::AccountId = whitelisted_caller();
		T::Currency::set_balance(
			&caller.clone(),
			T::Currency::minimum_balance().saturating_add(REGION_PRICE.into()),
		);

		let region = purchase_and_get_region_id::<T>(caller.clone())
			.expect("Offer not high enough for configuration.");

		// Worst case has an existing provisional pool assignment.
		Broker::<T>::do_pool(region, None, caller.clone(), Finality::Provisional)
			.map_err(|_| BenchmarkError::Weightless)?;

		#[extrinsic_call]
		_(RawOrigin::Signed(caller), region, 0x00000_fffff_fffff_00000.into());

		assert_last_event::<T>(
			Event::Interlaced {
				old_region_id: RegionId { begin: region.begin, core, mask: CoreMask::complete() },
				new_region_ids: (
					RegionId { begin: region.begin, core, mask: 0x00000_fffff_fffff_00000.into() },
					RegionId {
						begin: region.begin,
						core,
						mask: CoreMask::complete() ^ 0x00000_fffff_fffff_00000.into(),
					},
				),
			}
			.into(),
		);

		Ok(())
	}

	#[benchmark]
	fn assign() -> Result<(), BenchmarkError> {
		let sale_data = setup_and_start_sale::<T>()?;
		let core = sale_data.first_core;

		advance_to::<T>(2);

		let caller: T::AccountId = whitelisted_caller();
		T::Currency::set_balance(
			&caller.clone(),
			T::Currency::minimum_balance().saturating_add(REGION_PRICE.into()),
		);

		let region = purchase_and_get_region_id::<T>(caller.clone())
			.expect("Offer not high enough for configuration.");

		// Worst case has an existing provisional pool assignment.
		Broker::<T>::do_pool(region, None, caller.clone(), Finality::Provisional)
			.map_err(|_| BenchmarkError::Weightless)?;

		#[extrinsic_call]
		_(RawOrigin::Signed(caller), region, 1000, Provisional);

		let workplan_key = (region.begin, region.core);
		assert!(Workplan::<T>::get(workplan_key).is_some());

		assert!(Regions::<T>::get(region).is_some());

		assert_last_event::<T>(
			Event::Assigned {
				region_id: RegionId { begin: region.begin, core, mask: CoreMask::complete() },
				task: 1000,
				duration: 3u32.into(),
			}
			.into(),
		);

		Ok(())
	}

	#[benchmark]
	fn pool() -> Result<(), BenchmarkError> {
		let sale_data = setup_and_start_sale::<T>()?;
		let core = sale_data.first_core;

		advance_to::<T>(2);

		let caller: T::AccountId = whitelisted_caller();
		T::Currency::set_balance(
			&caller.clone(),
			T::Currency::minimum_balance().saturating_add(REGION_PRICE.into()),
		);

		let region = purchase_and_get_region_id::<T>(caller.clone())
			.expect("Offer not high enough for configuration.");

		let recipient: T::AccountId = account("recipient", 0, SEED);

		#[extrinsic_call]
		_(RawOrigin::Signed(caller), region, recipient, Final);

		let workplan_key = (region.begin, region.core);
		assert!(Workplan::<T>::get(workplan_key).is_some());

		assert_last_event::<T>(
			Event::Pooled {
				region_id: RegionId { begin: region.begin, core, mask: CoreMask::complete() },
				duration: 3u32.into(),
			}
			.into(),
		);

		Ok(())
	}

	#[benchmark]
	fn claim_revenue(
		m: Linear<1, { new_config_record::<T>().region_length }>,
	) -> Result<(), BenchmarkError> {
		let sale_data = setup_and_start_sale::<T>()?;
		let core = sale_data.first_core;

		advance_to::<T>(2);

		let caller: T::AccountId = whitelisted_caller();
		T::Currency::set_balance(
			&caller.clone(),
			T::Currency::minimum_balance().saturating_add(REGION_PRICE.into()),
		);
		T::Currency::set_balance(
			&Broker::<T>::account_id(),
			T::Currency::minimum_balance().saturating_add(200_000_000u32.into()),
		);

		let region = purchase_and_get_region_id::<T>(caller.clone())
			.expect("Offer not high enough for configuration.");

		let recipient: T::AccountId = account("recipient", 0, SEED);
		T::Currency::set_balance(&recipient.clone(), T::Currency::minimum_balance());

		Broker::<T>::do_pool(region, None, recipient.clone(), Final)
			.map_err(|_| BenchmarkError::Weightless)?;

		let revenue = 10_000_000u32.into();
		InstaPoolHistory::<T>::insert(
			region.begin,
			InstaPoolHistoryRecord {
				private_contributions: 4u32.into(),
				system_contributions: 3u32.into(),
				maybe_payout: Some(revenue),
			},
		);

		#[extrinsic_call]
		_(RawOrigin::Signed(caller), region, m);

		assert!(InstaPoolHistory::<T>::get(region.begin).is_none());
		assert_last_event::<T>(
			Event::RevenueClaimPaid {
				who: recipient,
				amount: 200_000_000u32.into(),
				next: if m < new_config_record::<T>().region_length {
					Some(RegionId {
						begin: region.begin.saturating_add(m),
						core,
						mask: CoreMask::complete(),
					})
				} else {
					None
				},
			}
			.into(),
		);

		Ok(())
	}

	#[benchmark]
	fn purchase_credit() -> Result<(), BenchmarkError> {
		setup_and_start_sale::<T>()?;

		advance_to::<T>(2);

		let caller: T::AccountId = whitelisted_caller();
		T::Currency::set_balance(
			&caller.clone(),
			T::Currency::minimum_balance().saturating_add(T::MinimumCreditPurchase::get()),
		);
		T::Currency::set_balance(&Broker::<T>::account_id(), T::Currency::minimum_balance());

		let beneficiary: RelayAccountIdOf<T> = account("beneficiary", 0, SEED);

		#[extrinsic_call]
		_(RawOrigin::Signed(caller.clone()), T::MinimumCreditPurchase::get(), beneficiary.clone());

		assert_last_event::<T>(
			Event::CreditPurchased {
				who: caller,
				beneficiary,
				amount: T::MinimumCreditPurchase::get(),
			}
			.into(),
		);

		Ok(())
	}

	#[benchmark]
	fn drop_region() -> Result<(), BenchmarkError> {
		let sale_data = setup_and_start_sale::<T>()?;
		let core = sale_data.first_core;

		advance_to::<T>(2);

		let caller: T::AccountId = whitelisted_caller();
		T::Currency::set_balance(
			&caller.clone(),
			T::Currency::minimum_balance().saturating_add(REGION_PRICE.into()),
		);

		let region = purchase_and_get_region_id::<T>(caller.clone())
			.expect("Offer not high enough for configuration.");

		advance_to::<T>(
			(T::TimeslicePeriod::get() * (REGION_LENGTH * 4).into())
				.try_into()
				.ok()
				.unwrap(),
		);

		#[extrinsic_call]
		_(RawOrigin::Signed(caller), region);

		assert_last_event::<T>(
			Event::RegionDropped {
				region_id: RegionId { begin: region.begin, core, mask: CoreMask::complete() },
				duration: 3u32.into(),
			}
			.into(),
		);

		Ok(())
	}

	#[benchmark]
	fn drop_contribution() -> Result<(), BenchmarkError> {
		let sale_data = setup_and_start_sale::<T>()?;
		let core = sale_data.first_core;
		let region_len = Configuration::<T>::get().unwrap().region_length;

		advance_to::<T>(2);

		let caller: T::AccountId = whitelisted_caller();
		T::Currency::set_balance(
			&caller.clone(),
			T::Currency::minimum_balance().saturating_add(REGION_PRICE.into()),
		);

		let region = purchase_and_get_region_id::<T>(caller.clone())
			.expect("Offer not high enough for configuration.");

		let recipient: T::AccountId = account("recipient", 0, SEED);

		Broker::<T>::do_pool(region, None, recipient, Final)
			.map_err(|_| BenchmarkError::Weightless)?;

		advance_to::<T>(
			(T::TimeslicePeriod::get() * (region_len * 8).into()).try_into().ok().unwrap(),
		);

		#[extrinsic_call]
		_(RawOrigin::Signed(caller), region);

		assert_last_event::<T>(
			Event::ContributionDropped {
				region_id: RegionId { begin: region.begin, core, mask: CoreMask::complete() },
			}
			.into(),
		);

		Ok(())
	}

	#[benchmark]
	fn drop_history() -> Result<(), BenchmarkError> {
		setup_and_start_sale::<T>()?;
		let when = 5u32.into();
		let revenue = 10_000_000u32.into();
		let region_len = Configuration::<T>::get().unwrap().region_length;

		advance_to::<T>(
			(T::TimeslicePeriod::get() * (region_len * 8).into()).try_into().ok().unwrap(),
		);

		let caller: T::AccountId = whitelisted_caller();
		InstaPoolHistory::<T>::insert(
			when,
			InstaPoolHistoryRecord {
				private_contributions: 4u32.into(),
				system_contributions: 3u32.into(),
				maybe_payout: Some(revenue),
			},
		);

		#[extrinsic_call]
		_(RawOrigin::Signed(caller), when);

		assert!(InstaPoolHistory::<T>::get(when).is_none());
		assert_last_event::<T>(Event::HistoryDropped { when, revenue }.into());

		Ok(())
	}

	#[benchmark]
	fn drop_renewal() -> Result<(), BenchmarkError> {
		let sale_data = setup_and_start_sale::<T>()?;
		let core = sale_data.first_core;
		let when = 5u32.into();
		let region_len = Configuration::<T>::get().unwrap().region_length;

		advance_to::<T>(
			(T::TimeslicePeriod::get() * (region_len * 3).into()).try_into().ok().unwrap(),
		);

		let id = PotentialRenewalId { core, when };
		let record =
			PotentialRenewalRecord { completion: CompletionStatus::Complete(new_schedule()) };
		PotentialRenewals::<T>::insert(id, record);

		let caller: T::AccountId = whitelisted_caller();

		#[extrinsic_call]
		_(RawOrigin::Signed(caller), core, when);

		assert!(PotentialRenewals::<T>::get(id).is_none());
		assert_last_event::<T>(Event::PotentialRenewalDropped { core, when }.into());

		Ok(())
	}

	#[benchmark]
	fn request_core_count(n: Linear<0, { MAX_CORE_COUNT.into() }>) -> Result<(), BenchmarkError> {
		let admin_origin =
			T::AdminOrigin::try_successful_origin().map_err(|_| BenchmarkError::Weightless)?;

		#[extrinsic_call]
		_(admin_origin as T::RuntimeOrigin, n.try_into().unwrap());

		assert_last_event::<T>(
			Event::CoreCountRequested { core_count: n.try_into().unwrap() }.into(),
		);

		Ok(())
	}

	#[benchmark]
	fn process_core_count(n: Linear<0, { MAX_CORE_COUNT.into() }>) -> Result<(), BenchmarkError> {
		setup_and_start_sale::<T>()?;

		let core_count = n.try_into().unwrap();

		CoreCountInbox::<T>::put(core_count);

		let mut status = Status::<T>::get().ok_or(BenchmarkError::Weightless)?;

		#[block]
		{
			Broker::<T>::process_core_count(&mut status);
		}

		assert_last_event::<T>(Event::CoreCountChanged { core_count }.into());

		Ok(())
	}

	#[benchmark]
	fn process_revenue() -> Result<(), BenchmarkError> {
		setup_and_start_sale::<T>()?;

		advance_to::<T>(2);

		let caller: T::AccountId = whitelisted_caller();
		T::Currency::set_balance(
			&caller.clone(),
			T::Currency::minimum_balance().saturating_add(30_000_000u32.into()),
		);
		T::Currency::set_balance(
			&Broker::<T>::account_id(),
			T::Currency::minimum_balance().saturating_add(90_000_000u32.into()),
		);

		let timeslice_period: u32 = T::TimeslicePeriod::get().try_into().ok().unwrap();
		let multiplicator = 5;

		RevenueInbox::<T>::put(OnDemandRevenueRecord {
			until: (timeslice_period * multiplicator).into(),
			amount: 10_000_000u32.into(),
		});

		let timeslice = multiplicator - 1;
		InstaPoolHistory::<T>::insert(
			timeslice,
			InstaPoolHistoryRecord {
				private_contributions: 4u32.into(),
				system_contributions: 6u32.into(),
				maybe_payout: None,
			},
		);

		#[block]
		{
			Broker::<T>::process_revenue();
		}

		assert_last_event::<T>(
			Event::ClaimsReady {
				when: timeslice.into(),
				system_payout: 6_000_000u32.into(),
				private_payout: 4_000_000u32.into(),
			}
			.into(),
		);

		Ok(())
	}

	#[benchmark]
	fn request_revenue_info_at() {
		let current_timeslice = Broker::<T>::current_timeslice();
		let rc_block = T::TimeslicePeriod::get() * current_timeslice.into();

		#[block]
		{
			T::Coretime::request_revenue_info_at(rc_block);
		}
	}

	#[benchmark]
	fn notify_core_count() -> Result<(), BenchmarkError> {
		let admin_origin =
			T::AdminOrigin::try_successful_origin().map_err(|_| BenchmarkError::Weightless)?;

		#[extrinsic_call]
		_(admin_origin as T::RuntimeOrigin, 100);

		assert!(CoreCountInbox::<T>::take().is_some());
		Ok(())
	}

	#[benchmark]
	fn notify_revenue() -> Result<(), BenchmarkError> {
		let admin_origin =
			T::AdminOrigin::try_successful_origin().map_err(|_| BenchmarkError::Weightless)?;

		#[extrinsic_call]
		_(
			admin_origin as T::RuntimeOrigin,
			OnDemandRevenueRecord { until: 100u32.into(), amount: 100_000_000u32.into() },
		);

		assert!(RevenueInbox::<T>::take().is_some());
		Ok(())
	}

	#[benchmark]
	fn do_tick_base() -> Result<(), BenchmarkError> {
		setup_and_start_sale::<T>()?;

		advance_to::<T>(5);

		let mut status = Status::<T>::get().unwrap();
		status.last_committed_timeslice = 3;
		Status::<T>::put(&status);

		#[block]
		{
			Broker::<T>::do_tick();
		}

		let updated_status = Status::<T>::get().unwrap();
		assert_eq!(status, updated_status);

		Ok(())
	}

	#[benchmark]
	fn force_reserve() -> Result<(), BenchmarkError> {
		Configuration::<T>::put(new_config_record::<T>());
		// Assume Reservations to be almost filled for worst case.
		let reservation_count = T::MaxReservedCores::get().saturating_sub(1);
		setup_reservations::<T>(reservation_count);

		// Assume leases to be filled for worst case
		setup_leases::<T>(T::MaxLeasedCores::get(), 1, 10);

		let origin =
			T::AdminOrigin::try_successful_origin().map_err(|_| BenchmarkError::Weightless)?;

		// Sales must be started.
		Broker::<T>::do_start_sales(
			Default::default(),
			CoreIndex::try_from(reservation_count).unwrap(),
		)
		.map_err(|_| BenchmarkError::Weightless)?;

		// Add a core.
		let core_count = Status::<T>::get().unwrap().core_count;
		Broker::<T>::do_request_core_count(core_count + 1).unwrap();

		advance_to::<T>(T::TimeslicePeriod::get().try_into().ok().unwrap());

		let schedule = new_schedule();

		#[extrinsic_call]
		_(origin as T::RuntimeOrigin, schedule.clone(), core_count);

		assert_eq!(Reservations::<T>::decode_len().unwrap(), T::MaxReservedCores::get() as usize);
		assert_eq!(ForceReservations::<T>::get(), vec![schedule]);

		Ok(())
	}

	#[benchmark]
	fn swap_leases() -> Result<(), BenchmarkError> {
		let admin_origin =
			T::AdminOrigin::try_successful_origin().map_err(|_| BenchmarkError::Weightless)?;

		// Add two leases in `Leases`
		let n = (T::MaxLeasedCores::get() / 2) as usize;
		let mut leases = vec![LeaseRecordItem { task: 1, until: 10u32.into() }; n];
		leases.extend(vec![LeaseRecordItem { task: 2, until: 20u32.into() }; n]);
		Leases::<T>::put(BoundedVec::try_from(leases).unwrap());

		#[extrinsic_call]
		_(admin_origin as T::RuntimeOrigin, 1, 2);

		Ok(())
	}

	#[benchmark]
	fn enable_auto_renew() -> Result<(), BenchmarkError> {
		setup_and_start_sale::<T>()?;

		advance_to::<T>(2);

		let region_end =
			T::CoretimeMarket::get_region_end().map_err(|_| BenchmarkError::Weightless)?;
		// We assume max auto renewals for worst case.
		(0..T::MaxAutoRenewals::get() - 1).try_for_each(|indx| -> Result<(), BenchmarkError> {
			let task = 1000 + indx;
			let caller: T::AccountId = T::SovereignAccountOf::maybe_convert(task)
				.expect("Failed to get sovereign account");
			// Sovereign account needs sufficient funds to purchase and renew.
			T::Currency::set_balance(
				&caller.clone(),
				T::Currency::minimum_balance().saturating_add(REGION_PRICE.into()),
			);

			let region = purchase_and_get_region_id::<T>(caller.clone())
				.expect("Offer not high enough for configuration.");

			Broker::<T>::do_assign(region, None, task, Final)
				.map_err(|_| BenchmarkError::Weightless)?;

			Broker::<T>::do_enable_auto_renew(caller, region.core, task, Some(region_end))?;

			Ok(())
		})?;

		let caller: T::AccountId =
			T::SovereignAccountOf::maybe_convert(2001).expect("Failed to get sovereign account");
		// Sovereign account needs sufficient funds to purchase and renew.
		T::Currency::set_balance(
			&caller.clone(),
			T::Currency::minimum_balance()
				.saturating_add(REGION_PRICE.into())
				.saturating_add(REGION_RENEWAL_PRICE.into()),
		);

		// The region for which we benchmark enable auto renew.
		let region = purchase_and_get_region_id::<T>(caller.clone())
			.expect("Offer not high enough for configuration.");
		Broker::<T>::do_assign(region, None, 2001, Final)
			.map_err(|_| BenchmarkError::Weightless)?;

		// The most 'intensive' path is when we renew the core upon enabling auto-renewal.
		// Therefore, we advance to next bulk sale:
		let timeslice_period: u32 = T::TimeslicePeriod::get().try_into().ok().unwrap();
		advance_to::<T>(REGION_LENGTH * timeslice_period);

		#[extrinsic_call]
		_(RawOrigin::Signed(caller), region.core, 2001, None);

		assert_last_event::<T>(Event::AutoRenewalEnabled { core: region.core, task: 2001 }.into());
		// Make sure we indeed renewed:
		let region_end =
			T::CoretimeMarket::get_region_end().map_err(|_| BenchmarkError::Weightless)?;
		assert!(PotentialRenewals::<T>::get(PotentialRenewalId {
			core: region.core,
			when: region_end,
		})
		.is_some());

		Ok(())
	}

	#[benchmark]
	fn disable_auto_renew() -> Result<(), BenchmarkError> {
		let sale_data = setup_and_start_sale::<T>()?;
		let core = sale_data.first_core;

		advance_to::<T>(2);

		let region_end =
			T::CoretimeMarket::get_region_end().map_err(|_| BenchmarkError::Weightless)?;
		// We assume max auto renewals for worst case.
		(0..T::MaxAutoRenewals::get()).try_for_each(|indx| -> Result<(), BenchmarkError> {
			let task = 1000 + indx;
			let caller: T::AccountId = T::SovereignAccountOf::maybe_convert(task)
				.expect("Failed to get sovereign account");
			T::Currency::set_balance(
				&caller.clone(),
				T::Currency::minimum_balance().saturating_add(REGION_PRICE.into()),
			);

			let region = purchase_and_get_region_id::<T>(caller.clone())
				.expect("Offer not high enough for configuration.");

			Broker::<T>::do_assign(region, None, task, Final)
				.map_err(|_| BenchmarkError::Weightless)?;

			Broker::<T>::do_enable_auto_renew(caller, region.core, task, Some(region_end))?;

			Ok(())
		})?;

		let task = 1000;

		let caller: T::AccountId =
			T::SovereignAccountOf::maybe_convert(task).expect("Failed to get sovereign account");

		#[extrinsic_call]
		_(RawOrigin::Signed(caller), core, task);

		assert_last_event::<T>(Event::AutoRenewalDisabled { core, task }.into());

		Ok(())
	}

	#[benchmark]
	fn on_new_timeslice() -> Result<(), BenchmarkError> {
		advance_to::<T>(2);

		let caller: T::AccountId = whitelisted_caller();
		T::Currency::set_balance(
			&caller.clone(),
			T::Currency::minimum_balance().saturating_add(REGION_PRICE.into()),
		);

		let _region = purchase_and_get_region_id::<T>(caller.clone())
			.expect("Offer not high enough for configuration.");

		let timeslice = Broker::<T>::current_timeslice();

		#[block]
		{
			T::Coretime::on_new_timeslice(timeslice);
		}

		Ok(())
	}

	#[benchmark]
	fn remove_assignment() -> Result<(), BenchmarkError> {
		setup_and_start_sale::<T>()?;

		advance_to::<T>(2);

		let caller: T::AccountId = whitelisted_caller();
		T::Currency::set_balance(
			&caller.clone(),
			T::Currency::minimum_balance().saturating_add(REGION_PRICE.into()),
		);

		let region = purchase_and_get_region_id::<T>(caller.clone())
			.expect("Offer not high enough for configuration.");

		Broker::<T>::do_assign(region, None, 1000, Provisional)
			.map_err(|_| BenchmarkError::Weightless)?;

		let origin =
			T::AdminOrigin::try_successful_origin().map_err(|_| BenchmarkError::Weightless)?;

		#[extrinsic_call]
		_(origin as T::RuntimeOrigin, region);

		Ok(())
	}

	#[benchmark]
	fn remove_potential_renewal() -> Result<(), BenchmarkError> {
		advance_to::<T>(2);

		let caller: T::AccountId = whitelisted_caller();
		T::Currency::set_balance(
			&caller.clone(),
			T::Currency::minimum_balance().saturating_add(REGION_PRICE.into()),
		);

		let region_id = purchase_and_get_region_id::<T>(caller.clone())
			.expect("Offer not high enough for configuration.");
		let region = Regions::<T>::get(region_id).expect("Requested region not found");

		Broker::<T>::do_assign(region_id, None, 1000, Final)
			.map_err(|_| BenchmarkError::Weightless)?;

		let origin =
			T::AdminOrigin::try_successful_origin().map_err(|_| BenchmarkError::Weightless)?;

		#[extrinsic_call]
		_(origin as T::RuntimeOrigin, region_id.core, region.end);

		assert_last_event::<T>(
			Event::PotentialRenewalRemoved { core: region_id.core, timeslice: region.end }.into(),
		);

		Ok(())
	}

	#[benchmark]
	fn process_tick_action_renew_region() {
		#[block]
		{
			// TODO #10900. This benchmark is useless for the current implementation of the market
			// logic but will be useful with RFC-17 implementation.
		}
	}

	#[benchmark]
	fn process_tick_action_sell_region() {
		let owner: T::AccountId = whitelisted_caller();
		let action = TickAction::SellRegion {
			owner: owner.clone(),
			paid: T::Currency::minimum_balance(),
			region_id: RegionId { begin: 0, core: 0, mask: CoreMask::complete() },
			region_end: 1,
		};
		let mut meter = WeightMeter::new();

		#[block]
		{
			Broker::<T>::process_tick_action(action, &mut meter);
		}

		assert_last_event::<T>(
			Event::Purchased {
				who: owner,
				region_id: RegionId { begin: 0, core: 0, mask: CoreMask::complete() },
				price: T::Currency::minimum_balance(),
				duration: 1,
			}
			.into(),
		);
	}

	#[benchmark]
	fn process_tick_action_refund() {
		#[block]
		{
			// TODO #10900. This benchmark is useless for the current implementation of the market
			// logic but will be useful with RFC-17 implementation.
		}
	}

	// #[benchmark]
	// fn process_tick_action_sale_rotated(
	// 	n: Linear<0, { MAX_CORE_COUNT.into() }>,
	// ) -> Result<(), BenchmarkError> {
	// 	let config = new_config_record::<T>();
	// 	Configuration::<T>::put(config.clone());

	// 	// Ensure there is one buyable core then use the rest to max out reservations and leases, if
	// 	// possible for worst case.

	// 	// First allocate up to MaxReservedCores for reservations
	// 	let n_reservations = T::MaxReservedCores::get().min(n.saturating_sub(1));
	// 	setup_reservations::<T>(n_reservations);
	// 	// Then allocate remaining cores to leases, up to MaxLeasedCores
	// 	let n_leases =
	// 		T::MaxLeasedCores::get().min(n.saturating_sub(1).saturating_sub(n_reservations));
	// 	setup_leases::<T>(n_leases, 1, 20);

	// 	// Start sales so we can test the auto-renewals.
	// 	Broker::<T>::do_start_sales(
	// 		Default::default(),
	// 		n.saturating_sub(n_reservations)
	// 			.saturating_sub(n_leases)
	// 			.try_into()
	// 			.expect("Upper limit of n is a u16."),
	// 	)
	// 	.expect("Configuration was initialized before; qed");

	// 	// Advance to the fixed price period.
	// 	advance_to::<T>(2);

	// 	// Assume max auto renewals for worst case. This is between 1 and the value of
	// 	// MaxAutoRenewals.
	// 	let n_renewable = T::MaxAutoRenewals::get()
	// 		.min(n.saturating_sub(n_leases).saturating_sub(n_reservations));

	// 	let timeslice_period: u32 = T::TimeslicePeriod::get().try_into().ok().unwrap();

	// 	let now = RCBlockNumberProviderOf::<T::Coretime>::current_block_number();
	// 	let price = REGION_PRICE.into();
	// 	(0..n_renewable.into()).try_for_each(|indx| -> Result<(), BenchmarkError> {
	// 		let task = 1000 + indx;
	// 		let caller: T::AccountId = T::SovereignAccountOf::maybe_convert(task)
	// 			.expect("Failed to get sovereign account");
	// 		T::Currency::set_balance(
	// 			&caller.clone(),
	// 			T::Currency::minimum_balance().saturating_add(price).saturating_add(price),
	// 		);

	// 		let region = purchase_and_get_region_id::<T>(caller.clone(), start_price)
	// 			.expect("Offer not high enough for configuration.");

	// 		Broker::<T>::do_assign(region, None, task, Final)
	// 			.map_err(|_| BenchmarkError::Weightless)?;

	// 		Broker::<T>::do_enable_auto_renew(caller, region.core, task, Some(sale.region_end))?;

	// 		Ok(())
	// 	})?;

	// 	// Advance to the block before the rotate_sale in which the auto-renewals will take place.
	// 	let rotate_block = timeslice_period.saturating_mul(config.region_length) - 2;
	// 	advance_to::<T>(rotate_block - 1);

	// 	// Advance one block and manually tick so we can isolate the `rotate_sale` call.
	// 	System::<T>::set_block_number(rotate_block.into());
	// 	RCBlockNumberProviderOf::<T::Coretime>::set_block_number(rotate_block.into());
	// 	let mut status = Status::<T>::get().expect("Sale has started.");
	// 	let sale = SaleInfo::<T>::get().expect("Sale has started.");
	// 	Broker::<T>::process_core_count(&mut status);
	// 	Broker::<T>::process_revenue();
	// 	status.last_committed_timeslice = config.region_length;
	// 	Status::<T>::put(status.clone());

	// 	let block = RCBlockNumberProviderOf::<T::Coretime>::current_block_number();
	// 	let reserved_cores = <Broker<T> as Market<T>>::CoreCount::reserved_core_count();
	// 	let (new_prices, new_sale) = T::CoretimeMarket::rotate
	// 		market::rotate_sale::<T>(&sale, &config, &status, reserved_cores, block);
	// 	SaleInfo::<T>::put(new_sale.clone());
	// 	let action = TickAction::SaleRotated { old_sale: sale.clone(), new_sale };

	// 	let mut meter = WeightMeter::new();

	// 	#[block]
	// 	{
	// 		Broker::<T>::process_tick_action(action, &mut meter);
	// 	}

	// 	// Get prices from the actual price adapter.
	// 	// let new_sale = SaleInfo::<T>::get().expect("Sale has started.");
	// 	let now = RCBlockNumberProviderOf::<T::Coretime>::current_block_number();
	// 	// let sale_start = config.interlude_length.saturating_add(rotate_block.into());

	// 	assert_has_event::<T>(
	// 		Event::SaleInitialized {
	// 			sale_start: now,
	// 			region_begin: sale.region_begin + config.region_length,
	// 			region_end: sale.region_end + config.region_length,
	// 			cores_offered: n
	// 				.saturating_sub(n_reservations)
	// 				.saturating_sub(n_leases)
	// 				.try_into()
	// 				.unwrap(),
	// 		}
	// 		.into(),
	// 	);

	// 	// Make sure all cores got renewed:
	// 	(0..n_renewable).for_each(|indx| {
	// 		let task = 1000 + indx;
	// 		let who = T::SovereignAccountOf::maybe_convert(task)
	// 			.expect("Failed to get sovereign account");
	// 		assert_has_event::<T>(
	// 			Event::Renewed {
	// 				who,
	// 				old_core: n_reservations as u16 + n_leases as u16 + indx as u16,
	// 				core: n_reservations as u16 + n_leases as u16 + indx as u16,
	// 				price,
	// 				begin: new_sale.region_begin,
	// 				duration: config.region_length,
	// 				workload: Schedule::truncate_from(vec![ScheduleItem {
	// 					assignment: Task(task),
	// 					mask: CoreMask::complete(),
	// 				}]),
	// 			}
	// 			.into(),
	// 		);
	// 	});

	// 	Ok(())
	// }

	// #[benchmark]
	// fn market_sale_rotated() -> Result<(), BenchmarkError> {
	// 	setup_and_start_sale::<T>()?;

	// 	let config = new_config_record::<T>();
	// 	let sale = SaleInfo::<T>::get().expect("Sale should be present at this point");
	// 	let status = StatusRecord {
	// 		core_count: 0,
	// 		private_pool_size: 0,
	// 		system_pool_size: 0,
	// 		last_committed_timeslice: 0,
	// 		last_timeslice: 0,
	// 	};
	// 	let block_number = RCBlockNumberProviderOf::<T::Coretime>::current_block_number();
	// 	let mut actions = vec![];

	// 	#[block]
	// 	{
	// 		market::sale_rotated::<T, Broker<T>>(
	// 			sale,
	// 			&config,
	// 			&status,
	// 			block_number,
	// 			&mut actions,
	// 		);
	// 	}

	// 	assert_eq!(actions.len(), 1);
	// 	assert!(matches!(actions[0], TickAction::SaleRotated { .. }));

	// 	Ok(())
	// }

	#[benchmark]
	fn force_transfer() -> Result<(), BenchmarkError> {
		setup_and_start_sale::<T>()?;
		advance_to::<T>(2);

		let caller: T::AccountId = whitelisted_caller();
		T::Currency::set_balance(
			&caller.clone(),
			T::Currency::minimum_balance().saturating_add(REGION_PRICE.into()),
		);

		let region = purchase_and_get_region_id::<T>(caller.clone())
			.map_err(|_| BenchmarkError::Weightless)?;

		let recipient: T::AccountId = account("recipient", 0, SEED);

		let origin =
			T::AdminOrigin::try_successful_origin().map_err(|_| BenchmarkError::Weightless)?;

		#[extrinsic_call]
		_(origin, region, recipient.clone());

		assert_last_event::<T>(
			Event::Transferred {
				region_id: region,
				old_owner: Some(caller),
				owner: Some(recipient),
				duration: 3u32.into(),
			}
			.into(),
		);

		Ok(())
	}

	// Implements a test for each benchmark. Execute with:
	// `cargo test -p pallet-broker --features runtime-benchmarks`.
	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test);
}
