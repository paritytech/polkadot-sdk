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

//! Edit and manage referenda voring tracks.
#![cfg_attr(not(feature = "std"), no_std)]

mod benchmarking;
pub mod weights;

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

use core::iter::Map;
use frame_support::{storage::PrefixIterator, traits::OriginTrait};
use frame_system::pallet_prelude::BlockNumberFor;
use pallet_referenda::{BalanceOf, PalletsOriginOf, Track, TrackIdOf, TrackInfoOf};
use sp_core::Get;
use sp_std::{borrow::Cow, vec::Vec};

pub use pallet::*;
pub use weights::WeightInfo;

type TrackOf<T, I> = Track<<T as Config<I>>::TrackId, BalanceOf<T, I>, BlockNumberFor<T>>;

type TracksIter<T, I> = Map<
	PrefixIterator<(<T as Config<I>>::TrackId, TrackInfoOf<T, I>)>,
	fn((<T as Config<I>>::TrackId, TrackInfoOf<T, I>)) -> Cow<'static, TrackOf<T, I>>,
>;

impl<T: Config<I>, I> pallet_referenda::TracksInfo<BalanceOf<T, I>, BlockNumberFor<T>>
	for Pallet<T, I>
{
	type Id = T::TrackId;
	type RuntimeOrigin = <T::RuntimeOrigin as OriginTrait>::PalletsOrigin;
	type TracksIter = TracksIter<T, I>;

	fn tracks() -> Self::TracksIter {
		Tracks::<T, I>::iter().map(|(id, info)| Cow::Owned(Track { id, info }))
	}
	fn track_for(origin: &Self::RuntimeOrigin) -> Result<Self::Id, ()> {
		OriginToTrackId::<T, I>::get(origin).ok_or(())
	}
	fn tracks_ids() -> Vec<Self::Id> {
		TracksIds::<T, I>::get().into_inner()
	}
	fn info(id: Self::Id) -> Option<Cow<'static, TrackInfoOf<T, I>>> {
		Tracks::<T, I>::get(id).map(Cow::Owned)
	}
}

impl<T: Config<I>, I: 'static> Get<Vec<TrackOf<T, I>>> for crate::Pallet<T, I> {
	fn get() -> Vec<Track<T::TrackId, BalanceOf<T, I>, BlockNumberFor<T>>> {
		// expensive but it doesn't seem to be used anywhere
		<Pallet<T, I> as pallet_referenda::TracksInfo<BalanceOf<T, I>, BlockNumberFor<T>>>::tracks()
			.map(|t| t.into_owned())
			.collect()
	}
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;

	#[pallet::config]
	pub trait Config<I: 'static = ()>: frame_system::Config + pallet_referenda::Config<I> {
		type RuntimeEvent: From<Event<Self, I>>
			+ IsType<<Self as frame_system::Config>::RuntimeEvent>;

		type TrackId: Parameter + Member + Copy + MaxEncodedLen + Ord;

		type MaxTracks: Get<u32>;
		// type WeightInfo: WeightInfo;
	}

	#[pallet::pallet]
	pub struct Pallet<T, I = ()>(_);

	#[pallet::call]
	impl<T: Config<I>, I: 'static> Pallet<T, I> {
		#[pallet::call_index(0)]
		#[pallet::weight(Weight::from_parts(10_000, 0) + T::DbWeight::get().writes(1))]
		pub fn udpate(origin: OriginFor<T>, _id: TrackIdOf<T, I>) -> DispatchResultWithPostInfo {
			let _sender = ensure_signed(origin)?;
			// Self::deposit_event(Event::Foo { sender });
			Ok(().into())
		}
	}

	#[pallet::storage]
	pub type TracksIds<T: Config<I>, I: 'static = ()> =
		StorageValue<_, BoundedVec<T::TrackId, <T as Config<I>>::MaxTracks>, ValueQuery>;

	#[pallet::storage]
	pub type OriginToTrackId<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Blake2_128Concat, PalletsOriginOf<T>, T::TrackId>;

	#[pallet::storage]
	pub type Tracks<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Blake2_128Concat, T::TrackId, TrackInfoOf<T, I>>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config<I>, I: 'static = ()> {
		// Foo(T::AccountId),
	}

	#[pallet::error]
	pub enum Error<T, I = ()> {}
}
