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

//! Common try-runtime only tests for runtimes.

use frame_support::traits::{Get, Hooks};
use pallet_fast_unstake::{Pallet as FastUnstake, *};

/// progress until the inactive nominators have all beenprocessed.
pub fn migrate_all_inactive_nominators<T: pallet_fast_unstake::Config + pallet_staking::Config>()
where
	<T as frame_system::Config>::RuntimeEvent: TryInto<pallet_fast_unstake::Event<T>>,
{
	let mut unstaked_ok = 0;
	let mut unstaked_err = 0;
	let mut unstaked_slashed = 0;

	log::info!(
		target: "runtime::test",
		"registered {} successfully, starting at {:?}.",
		Queue::<T>::count(),
		frame_system::Pallet::<T>::block_number(),
	);
	while Queue::<T>::count() != 0 || Head::<T>::get().is_some() {
		let now = frame_system::Pallet::<T>::block_number();
		let weight = <T as frame_system::Config>::BlockWeights::get().max_block;
		let consumed = FastUnstake::<T>::on_idle(now, weight);
		log::debug!(target: "runtime::test", "consumed {:?} ({})", consumed, consumed.ref_time() as f32 / weight.ref_time() as f32);

		frame_system::Pallet::<T>::read_events_no_consensus()
			.into_iter()
			.map(|r| r.event)
			.filter_map(|e| {
				let maybe_fast_unstake_event: Option<pallet_fast_unstake::Event<T>> =
					e.try_into().ok();
				maybe_fast_unstake_event
			})
			.for_each(|e: pallet_fast_unstake::Event<T>| match e {
				pallet_fast_unstake::Event::<T>::Unstaked { result, .. } =>
					if result.is_ok() {
						unstaked_ok += 1;
					} else {
						unstaked_err += 1
					},
				pallet_fast_unstake::Event::<T>::Slashed { .. } => unstaked_slashed += 1,
				pallet_fast_unstake::Event::<T>::InternalError => unreachable!(),
				_ => {},
			});

		if now % 100u32.into() == sp_runtime::traits::Zero::zero() {
			log::info!(
				target: "runtime::test",
				"status: ok {}, err {}, slash {}",
				unstaked_ok,
				unstaked_err,
				unstaked_slashed,
			);
		}

		frame_system::Pallet::<T>::reset_events();
	}
}
