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

//! Storage migrations for the broker pallet.

use crate::*;
use frame_support::{pallet_prelude::*, storage_alias, traits::OnRuntimeUpgrade, BoundedVec};
/// The log target.
const TARGET: &'static str = "runtime::broker::migration::v1";

/// The original data layout of the broker pallet without a specific version number.
pub mod v0 {
	use super::*;
	use frame_system::pallet_prelude::BlockNumberFor;
	use sp_arithmetic::Perbill;

	#[storage_alias]
	pub type Configuration<T: Config> = StorageValue<Pallet<T>, ConfigRecordOf<T>, OptionQuery>;
	pub type ConfigRecordOf<T> = ConfigRecord<BlockNumberFor<T>, RelayBlockNumberOf<T>>;

	#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
	pub struct ConfigRecord<BlockNumber, RelayBlockNumber> {
		/// The number of Relay-chain blocks in advance which scheduling should be fixed and the
		/// `Coretime::assign` API used to inform the Relay-chain.
		pub advance_notice: RelayBlockNumber,
		/// The length in blocks of the Interlude Period for forthcoming sales.
		pub interlude_length: BlockNumber,
		/// The length in blocks of the Leadin Period for forthcoming sales.
		pub leadin_length: BlockNumber,
		/// The length in timeslices of Regions which are up for sale in forthcoming sales.
		pub region_length: Timeslice,
		/// The proportion of cores available for sale which should be sold in order for the price
		/// to remain the same in the next sale.
		pub ideal_bulk_proportion: Perbill,
		/// An artificial limit to the number of cores which are allowed to be sold. If `Some` then
		/// no more cores will be sold than this.
		pub limit_cores_offered: Option<CoreIndex>,
		/// The amount by which the renewal price increases each sale period.
		pub renewal_bump: Perbill,
		/// The duration by which rewards for contributions to the InstaPool must be collected.
		pub contribution_timeout: Timeslice,
	}

	#[storage_alias]
	pub type SaleInfo<T: Config> = StorageValue<Pallet<T>, SaleInfoRecordOf<T>, OptionQuery>;
	pub type SaleInfoRecordOf<T> = SaleInfoRecord<BalanceOf<T>, BlockNumberFor<T>>;
}

pub mod v1 {
	use super::*;
	use frame_system::pallet_prelude::BlockNumberFor;

	pub struct Migration<T>(core::marker::PhantomData<T>);

	impl<T> OnRuntimeUpgrade for Migration<T>
	where
		T: Config,
		RelayBlockNumberOf<T>: TryFrom<BlockNumberFor<T>>,
	{
		#[cfg(feature = "try-runtime")]
		fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
			ensure!(StorageVersion::get::<Pallet<T>>() == 0, "can only upgrade from version 0");

			let (interlude_length, configuration_leadin_length) =
				if let Some(config_record) = v0::Configuration::<T>::get() {
					(config_record.interlude_length, config_record.leadin_length)
				} else {
					((0 as u32).into(), (0 as u32).into())
				};

			log::info!(target: TARGET, "Configuration Pre-Migration: Interlude Length {:?} Leading Length {:?} ", interlude_length, configuration_leadin_length);

			let (sale_start, sale_info_leadin_length) =
				if let Some(sale_info_record) = v0::SaleInfo::<T>::get() {
					(sale_info_record.sale_start, sale_info_record.leadin_length)
				} else {
					((0 as u32).into(), (0 as u32).into())
				};

			log::info!(target: TARGET, "SaleInfo Pre-Migration: Sale Start {:?} Interlude Length {:?}  ", sale_start, sale_info_leadin_length);

			Ok((interlude_length, configuration_leadin_length, sale_start, sale_info_leadin_length)
				.encode())
		}

		fn on_runtime_upgrade() -> Weight {
			let mut weight = T::DbWeight::get().reads(1);

			if StorageVersion::get::<Pallet<T>>() != 0 {
				log::warn!(
					target: TARGET,
					"skipping on_runtime_upgrade: executed on wrong storage version.\
				Expected version 0"
				);
				return weight
			}

			// using a u32::MAX as sentinel value in case TryFrom fails.
			// Ref: https://github.com/paritytech/polkadot-sdk/pull/3331#discussion_r1499014975

			if let Some(config_record) = v0::Configuration::<T>::take() {
				log::info!(target: TARGET, "migrating Configuration record");

				let updated_interlude_length: RelayBlockNumberOf<T> =
					match TryFrom::try_from(config_record.interlude_length) {
						Ok(val) => val,
						Err(_) => u32::MAX.into(),
					};

				let updated_leadin_length: RelayBlockNumberOf<T> =
					match TryFrom::try_from(config_record.leadin_length) {
						Ok(val) => val,
						Err(_) => u32::MAX.into(),
					};

				let updated_config_record = ConfigRecord {
					interlude_length: updated_interlude_length,
					leadin_length: updated_leadin_length,
					advance_notice: config_record.advance_notice,
					region_length: config_record.region_length,
					ideal_bulk_proportion: config_record.ideal_bulk_proportion,
					limit_cores_offered: config_record.limit_cores_offered,
					renewal_bump: config_record.renewal_bump,
					contribution_timeout: config_record.contribution_timeout,
				};
				Configuration::<T>::put(updated_config_record);
			}
			weight.saturating_accrue(T::DbWeight::get().reads_writes(1, 1));

			if let Some(sale_info) = v0::SaleInfo::<T>::take() {
				log::info!(target: TARGET, "migrating SaleInfo record");

				let updated_sale_start: RelayBlockNumberOf<T> =
					match TryFrom::try_from(sale_info.sale_start) {
						Ok(val) => val,
						Err(_) => u32::MAX.into(),
					};

				let updated_leadin_length: RelayBlockNumberOf<T> =
					match TryFrom::try_from(sale_info.leadin_length) {
						Ok(val) => val,
						Err(_) => u32::MAX.into(),
					};

				let updated_sale_info = SaleInfoRecord {
					sale_start: updated_sale_start,
					leadin_length: updated_leadin_length,
					price: sale_info.price,
					region_begin: sale_info.region_begin,
					region_end: sale_info.region_end,
					ideal_cores_sold: sale_info.ideal_cores_sold,
					cores_offered: sale_info.cores_offered,
					first_core: sale_info.first_core,
					sellout_price: sale_info.sellout_price,
					cores_sold: sale_info.cores_sold,
				};
				SaleInfo::<T>::put(updated_sale_info);
			}

			StorageVersion::new(1).put::<Pallet<T>>();
			weight.saturating_add(T::DbWeight::get().reads_writes(1, 2))
		}

		#[cfg(feature = "try-runtime")]
		fn post_upgrade(state: Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
			use frame_system::pallet_prelude::BlockNumberFor;

			ensure!(StorageVersion::get::<Pallet<T>>() == 1, "must upgrade");

			let (
				old_interlude_length,
				old_configuration_leadin_length,
				old_sale_start,
				old_sale_info_leadin_length,
			): (BlockNumberFor<T>, BlockNumberFor<T>, BlockNumberFor<T>, BlockNumberFor<T>) =
				Decode::decode(&mut &state[..]).expect("pre_upgrade provides a valid state; qed");

			if let Some(config_record) = Configuration::<T>::get() {
				ensure!(
					verify_updated::<T>(
						old_configuration_leadin_length,
						config_record.leadin_length
					),
					"must migrate configuration leadin_length"
				);

				ensure!(
					verify_updated::<T>(old_interlude_length, config_record.interlude_length),
					"must migrate configuration interlude_length"
				);
			}

			if let Some(sale_info) = SaleInfo::<T>::get() {
				ensure!(
					verify_updated::<T>(old_sale_start, sale_info.sale_start),
					"must migrate sale info sale_start"
				);

				ensure!(
					verify_updated::<T>(old_sale_info_leadin_length, sale_info.leadin_length),
					"must migrate sale info leadin_length"
				);
			}

			Ok(())
		}
	}

	#[cfg(feature = "try-runtime")]
	fn verify_updated<T>(old_value: BlockNumberFor<T>, new_value: RelayBlockNumberOf<T>) -> bool
	where
		T: Config,
		RelayBlockNumberOf<T>: TryFrom<BlockNumberFor<T>>,
	{
		let val: RelayBlockNumberOf<T> = match TryFrom::try_from(old_value) {
			Ok(val) => val,
			Err(_) => u32::MAX.into(),
		};

		val == new_value
	}
}

#[cfg(test)]
#[cfg(feature = "try-runtime")]
mod test {
	use super::*;
	use crate::mock::{Test as T, TestExt};

	#[allow(deprecated)]
	#[test]
	fn migration_works() {
		TestExt::new().execute_with(|| {
			assert_eq!(StorageVersion::get::<Pallet<T>>(), 0);

			// Migrate.
			let state = v1::Migration::<T>::pre_upgrade().unwrap();
			let _weight = v1::Migration::<T>::on_runtime_upgrade();
			v1::Migration::<T>::post_upgrade(state).unwrap();

			assert_eq!(StorageVersion::get::<Pallet<T>>(), 1);
		})
	}
}
