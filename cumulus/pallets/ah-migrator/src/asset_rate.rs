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

impl<T: Config> Pallet<T> {
	pub fn do_receive_asset_rates(
		rates: Vec<(<T as pallet_asset_rate::Config>::AssetKind, FixedU128)>,
	) -> Result<(), Error<T>> {
		log::info!(target: LOG_TARGET, "Processing {} asset rates", rates.len());

		let count = rates.len() as u32;
		Self::deposit_event(Event::AssetRatesReceived { count });

		for rate in rates {
			Self::do_receive_asset_rate(rate)?;
		}

		log::info!(target: LOG_TARGET, "Processed {} asset rates", count);
		Self::deposit_event(Event::AssetRatesProcessed { count_good: count, count_bad: 0 });

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
