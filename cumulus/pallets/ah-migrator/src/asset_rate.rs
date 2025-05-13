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
use pallet_rc_migrator::asset_rate::AssetRateMigrator;

impl<T: Config> Pallet<T> {
	pub fn do_receive_asset_rates(
		rates: Vec<(<T as pallet_asset_rate::Config>::AssetKind, FixedU128)>,
	) -> Result<(), Error<T>> {
		log::info!(target: LOG_TARGET, "Processing {} asset rates", rates.len());

		let count = rates.len() as u32;
		Self::deposit_event(Event::BatchReceived { pallet: PalletEventName::AssetRates, count });

		for rate in rates {
			Self::do_receive_asset_rate(rate)?;
		}

		log::info!(target: LOG_TARGET, "Processed {} asset rates", count);
		Self::deposit_event(Event::BatchProcessed {
			pallet: PalletEventName::AssetRates,
			count_good: count,
			count_bad: 0,
		});

		Ok(())
	}

	pub fn do_receive_asset_rate(
		rate: (<T as pallet_asset_rate::Config>::AssetKind, FixedU128),
	) -> Result<(), Error<T>> {
		let (asset_kind, rate) = rate;
		log::debug!(target: LOG_TARGET, "Inserting asset rate for {:?}: {}", asset_kind, rate);
		ConversionRateToNative::<T>::insert(asset_kind, rate);
		Ok(())
	}
}

impl<T: Config> crate::types::AhMigrationCheck for AssetRateMigrator<T> {
	type RcPrePayload = Vec<(<T as pallet_asset_rate::Config>::AssetKind, FixedU128)>;
	type AhPrePayload = ();

	fn pre_check(_: Self::RcPrePayload) -> Self::AhPrePayload {
		// AH pre: Verify no entries are present
		assert!(
			ConversionRateToNative::<T>::iter().next().is_none(),
			"Assert storage 'AssetRate::ConversionRateToNative::ah_pre::empty'"
		);
	}

	fn post_check(rc_pre_payload: Self::RcPrePayload, _: Self::AhPrePayload) {
		let ah_entries: Vec<_> = ConversionRateToNative::<T>::iter().collect();

		// AH post: Verify number of entries is correct
		assert_eq!(
			rc_pre_payload.len(),
			ah_entries.len(),
			"Assert storage 'AssetRate::ConversionRateToNative::ah_post::length'"
		);

		// AH post: Verify entry values match
		// Assert storage "AssetRate::ConversionRateToNative::ah_post::correct"
		// Assert storage "AssetRate::ConversionRateToNative::ah_post::consistent"
		for (pre_entry, post_entry) in rc_pre_payload.iter().zip(ah_entries.iter()) {
			assert_eq!(
				pre_entry, post_entry,
				"Assert storage 'AssetRate::ConversionRateToNative::ah_post::correct'"
			);
		}
	}
}
