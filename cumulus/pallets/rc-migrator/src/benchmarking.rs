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

use crate::*;
use frame_benchmarking::v2::*;
use frame_support::traits::Currency;
use frame_system::{Account as SystemAccount, RawOrigin};

fn assert_last_event<T: Config>(generic_event: <T as Config>::RuntimeEvent) {
	frame_system::Pallet::<T>::assert_last_event(generic_event.into());
}

#[benchmarks]
pub mod benchmarks {
	use super::*;

	#[benchmark]
	fn withdraw_account() {
		let create_liquid_account = |n: u8| {
			let who: AccountId32 = [n; 32].into();
			let ed = <pallet_balances::Pallet<T> as Currency<_>>::minimum_balance();
			let _ = <pallet_balances::Pallet<T> as Currency<_>>::deposit_creating(&who, ed);
		};

		let n = 50;
		(0..n).for_each(|i| create_liquid_account(i));
		let last_key: AccountId32 = [n / 2; 32].into();

		RcMigratedBalance::<T>::mutate(|tracker| {
			tracker.kept = <<T as Config>::Currency as Currency<_>>::total_issuance();
		});

		#[block]
		{
			let (who, account_info) = SystemAccount::<T>::iter_from_key(last_key).next().unwrap();
			let mut ah_weight = WeightMeter::new();
			let batch_len = 0;
			let res = AccountsMigrator::<T>::withdraw_account(
				who,
				account_info,
				&mut ah_weight,
				batch_len,
			);
			assert!(res.unwrap().is_some());
		}
	}

	#[benchmark]
	fn force_set_stage() {
		let stage = MigrationStageOf::<T>::Scheduled { block_number: 1u32.into() };

		#[extrinsic_call]
		_(RawOrigin::Root, Box::new(stage.clone()));

		assert_last_event::<T>(
			Event::StageTransition { old: MigrationStageOf::<T>::Pending, new: stage }.into(),
		);
	}

	#[benchmark]
	fn schedule_migration() {
		let start_moment = DispatchTime::<BlockNumberFor<T>>::At(10u32.into());

		#[extrinsic_call]
		_(RawOrigin::Root, start_moment);

		assert_last_event::<T>(
			Event::StageTransition {
				old: MigrationStageOf::<T>::Pending,
				new: MigrationStageOf::<T>::Scheduled { block_number: 10u32.into() },
			}
			.into(),
		);
	}

	#[benchmark]
	fn start_data_migration() {
		#[extrinsic_call]
		_(RawOrigin::Root);

		assert_last_event::<T>(
			Event::StageTransition {
				old: MigrationStageOf::<T>::Pending,
				new: MigrationStageOf::<T>::Starting,
			}
			.into(),
		);
	}

	#[benchmark]
	fn send_chunked_xcm_and_track() {
		let mut batches = XcmBatch::new();
		batches.push(vec![0u8; (MAX_XCM_SIZE / 2 - 10) as usize]);
		batches.push(vec![1u8; (MAX_XCM_SIZE / 2 - 10) as usize]);
		#[block]
		{
			let res = Pallet::<T>::send_chunked_xcm_and_track(
				batches,
				|batch| types::AhMigratorCall::<T>::TestCall { data: batch },
				|_| Weight::from_all(1),
			);
			assert_eq!(res.unwrap(), 1);
		}
	}

	#[benchmark]
	fn update_ah_msg_processed_count() {
		let new_processed = 100;

		#[extrinsic_call]
		_(RawOrigin::Root, new_processed);

		let (sent, processed) = DmpDataMessageCounts::<T>::get();
		assert_eq!(processed, new_processed);
		assert_eq!(sent, 0);
	}

	#[cfg(feature = "std")]
	pub fn test_withdraw_account<T: Config>() {
		_withdraw_account::<T>(true /* enable checks */)
	}

	#[cfg(feature = "std")]
	pub fn test_force_set_stage<T: Config>() {
		_force_set_stage::<T>(true /* enable checks */);
	}

	#[cfg(feature = "std")]
	pub fn test_schedule_migration<T: Config>() {
		_schedule_migration::<T>(true /* enable checks */);
	}

	#[cfg(feature = "std")]
	pub fn test_start_data_migration<T: Config>() {
		_start_data_migration::<T>(true /* enable checks */);
	}

	#[cfg(feature = "std")]
	pub fn test_update_ah_msg_processed_count<T: Config>() {
		_update_ah_msg_processed_count::<T>(true /* enable checks */);
	}

	#[cfg(feature = "std")]
	pub fn test_send_chunked_xcm_and_track<T: Config>() {
		_send_chunked_xcm_and_track::<T>(true /* enable checks */);
	}
}
