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

use crate::*;
use pallet_asset_rate::ConversionRateToNative;
use sp_runtime::FixedU128;

pub struct AssetRateMigrator<T> {
	pub _phantom: PhantomData<T>,
}

impl<T: Config> PalletMigration for AssetRateMigrator<T> {
	type Key = <T as pallet_asset_rate::Config>::AssetKind;
	type Error = Error<T>;

	fn migrate_many(
		mut last_key: Option<Self::Key>,
		weight_counter: &mut WeightMeter,
	) -> Result<Option<Self::Key>, Self::Error> {
		log::info!(target: LOG_TARGET, "Migrating asset rates");
		let mut messages = XcmBatchAndMeter::new_from_config::<T>();

		loop {
			if weight_counter
				.try_consume(<T as frame_system::Config>::DbWeight::get().reads_writes(1, 1))
				.is_err() || weight_counter.try_consume(messages.consume_weight()).is_err()
			{
				log::info!("RC weight limit reached at batch length {}, stopping", messages.len());
				if messages.is_empty() {
					return Err(Error::OutOfWeight);
				} else {
					break;
				}
			}
			if T::MaxAhWeight::get()
				.any_lt(T::AhWeightInfo::receive_asset_rates((messages.len() + 1) as u32))
			{
				log::info!("AH weight limit reached at batch length {}, stopping", messages.len());
				if messages.is_empty() {
					return Err(Error::OutOfWeight);
				} else {
					break;
				}
			}
			if messages.len() > 10_000 {
				log::warn!(target: LOG_TARGET, "Weight allowed very big batch, stopping");
				break;
			}

			let mut iter = if let Some(last_key) = last_key {
				ConversionRateToNative::<T>::iter_from_key(last_key)
			} else {
				ConversionRateToNative::<T>::iter()
			};

			match iter.next() {
				Some((key, value)) => {
					log::debug!(target: LOG_TARGET, "Extracting asset rate for {:?}", &key);
					ConversionRateToNative::<T>::remove(&key);
					messages.push((key.clone(), value));
					last_key = Some(key);
				},
				None => {
					log::debug!(target: LOG_TARGET, "No more asset rates to migrate");
					last_key = None;
					break;
				},
			}
		}

		if !messages.is_empty() {
			Pallet::<T>::send_chunked_xcm_and_track(
				messages,
				|messages| types::AhMigratorCall::<T>::ReceiveAssetRates { asset_rates: messages },
				|len| T::AhWeightInfo::receive_asset_rates(len),
			)?;
		}

		Ok(last_key)
	}
}

impl<T: Config> crate::types::RcMigrationCheck for AssetRateMigrator<T> {
	type RcPrePayload = Vec<(<T as pallet_asset_rate::Config>::AssetKind, FixedU128)>;

	fn pre_check() -> Self::RcPrePayload {
		let entries: Vec<_> = ConversionRateToNative::<T>::iter().collect();

		// RC pre: Collect all entries
		entries
	}

	fn post_check(_: Self::RcPrePayload) {
		// RC post: Ensure that entries have been deleted
		assert!(
			ConversionRateToNative::<T>::iter().next().is_none(),
			"Assert storage 'AssetRate::ConversionRateToNative::rc_post::empty'"
		);
	}
}
