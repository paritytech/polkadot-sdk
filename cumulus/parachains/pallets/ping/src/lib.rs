// Copyright 2020-2021 Parity Technologies (UK) Ltd.
// This file is part of Cumulus.

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus.  If not, see <http://www.gnu.org/licenses/>.

//! Pallet to spam the XCM/UMP.

#![cfg_attr(not(feature = "std"), no_std)]

use cumulus_pallet_xcm::{ensure_sibling_para, Origin as CumulusOrigin};
use cumulus_primitives_core::ParaId;
use frame_support::{parameter_types, BoundedVec};
use frame_system::Config as SystemConfig;
use sp_runtime::traits::Saturating;
use sp_std::prelude::*;
use xcm::latest::prelude::*;

pub use pallet::*;

parameter_types! {
	const MaxParachains: u32 = 100;
	const MaxPayloadSize: u32 = 1024;
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;

	#[pallet::pallet]
	#[pallet::generate_store(pub(super) trait Store)]
	pub struct Pallet<T>(_);

	/// The module configuration trait.
	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// The overarching event type.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		type RuntimeOrigin: From<<Self as SystemConfig>::RuntimeOrigin>
			+ Into<Result<CumulusOrigin, <Self as Config>::RuntimeOrigin>>;

		/// The overarching call type; we assume sibling chains use the same type.
		type RuntimeCall: From<Call<Self>> + Encode;

		type XcmSender: SendXcm;
	}

	/// The target parachains to ping.
	#[pallet::storage]
	pub(super) type Targets<T: Config> = StorageValue<
		_,
		BoundedVec<(ParaId, BoundedVec<u8, MaxPayloadSize>), MaxParachains>,
		ValueQuery,
	>;

	/// The total number of pings sent.
	#[pallet::storage]
	pub(super) type PingCount<T: Config> = StorageValue<_, u32, ValueQuery>;

	/// The sent pings.
	#[pallet::storage]
	pub(super) type Pings<T: Config> =
		StorageMap<_, Blake2_128Concat, u32, T::BlockNumber, OptionQuery>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		PingSent(ParaId, u32, Vec<u8>, XcmHash, MultiAssets),
		Pinged(ParaId, u32, Vec<u8>),
		PongSent(ParaId, u32, Vec<u8>, XcmHash, MultiAssets),
		Ponged(ParaId, u32, Vec<u8>, T::BlockNumber),
		ErrorSendingPing(SendError, ParaId, u32, Vec<u8>),
		ErrorSendingPong(SendError, ParaId, u32, Vec<u8>),
		UnknownPong(ParaId, u32, Vec<u8>),
	}

	#[pallet::error]
	pub enum Error<T> {
		/// Too many parachains have been added as a target.
		TooManyTargets,
		/// The payload provided is too large, limit is 1024 bytes.
		PayloadTooLarge,
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_finalize(n: T::BlockNumber) {
			for (para, payload) in Targets::<T>::get().into_iter() {
				let seq = PingCount::<T>::mutate(|seq| {
					*seq += 1;
					*seq
				});
				match send_xcm::<T::XcmSender>(
					(Parent, Junction::Parachain(para.into())).into(),
					Xcm(vec![Transact {
						origin_kind: OriginKind::Native,
						require_weight_at_most: Weight::from_parts(1_000, 1_000),
						call: <T as Config>::RuntimeCall::from(Call::<T>::ping {
							seq,
							payload: payload.clone().to_vec(),
						})
						.encode()
						.into(),
					}]),
				) {
					Ok((hash, cost)) => {
						Pings::<T>::insert(seq, n);
						Self::deposit_event(Event::PingSent(
							para,
							seq,
							payload.to_vec(),
							hash,
							cost,
						));
					},
					Err(e) => {
						Self::deposit_event(Event::ErrorSendingPing(
							e,
							para,
							seq,
							payload.to_vec(),
						));
					},
				}
			}
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::call_index(0)]
		#[pallet::weight(0)]
		pub fn start(origin: OriginFor<T>, para: ParaId, payload: Vec<u8>) -> DispatchResult {
			ensure_root(origin)?;
			let payload = BoundedVec::<u8, MaxPayloadSize>::try_from(payload)
				.map_err(|_| Error::<T>::PayloadTooLarge)?;
			Targets::<T>::try_mutate(|t| {
				t.try_push((para, payload)).map_err(|_| Error::<T>::TooManyTargets)
			})?;
			Ok(())
		}

		#[pallet::call_index(1)]
		#[pallet::weight(0)]
		pub fn start_many(
			origin: OriginFor<T>,
			para: ParaId,
			count: u32,
			payload: Vec<u8>,
		) -> DispatchResult {
			ensure_root(origin)?;
			let bounded_payload = BoundedVec::<u8, MaxPayloadSize>::try_from(payload)
				.map_err(|_| Error::<T>::PayloadTooLarge)?;
			for _ in 0..count {
				Targets::<T>::try_mutate(|t| {
					t.try_push((para, bounded_payload.clone()))
						.map_err(|_| Error::<T>::TooManyTargets)
				})?;
			}
			Ok(())
		}

		#[pallet::call_index(2)]
		#[pallet::weight(0)]
		pub fn stop(origin: OriginFor<T>, para: ParaId) -> DispatchResult {
			ensure_root(origin)?;
			Targets::<T>::mutate(|t| {
				if let Some(p) = t.iter().position(|(p, _)| p == &para) {
					t.swap_remove(p);
				}
			});
			Ok(())
		}

		#[pallet::call_index(3)]
		#[pallet::weight(0)]
		pub fn stop_all(origin: OriginFor<T>, maybe_para: Option<ParaId>) -> DispatchResult {
			ensure_root(origin)?;
			if let Some(para) = maybe_para {
				Targets::<T>::mutate(|t| t.retain(|&(x, _)| x != para));
			} else {
				Targets::<T>::kill();
			}
			Ok(())
		}

		#[pallet::call_index(4)]
		#[pallet::weight(0)]
		pub fn ping(origin: OriginFor<T>, seq: u32, payload: Vec<u8>) -> DispatchResult {
			// Only accept pings from other chains.
			let para = ensure_sibling_para(<T as Config>::RuntimeOrigin::from(origin))?;

			Self::deposit_event(Event::Pinged(para, seq, payload.clone()));
			match send_xcm::<T::XcmSender>(
				(Parent, Junction::Parachain(para.into())).into(),
				Xcm(vec![Transact {
					origin_kind: OriginKind::Native,
					require_weight_at_most: Weight::from_parts(1_000, 1_000),
					call: <T as Config>::RuntimeCall::from(Call::<T>::pong {
						seq,
						payload: payload.clone(),
					})
					.encode()
					.into(),
				}]),
			) {
				Ok((hash, cost)) =>
					Self::deposit_event(Event::PongSent(para, seq, payload, hash, cost)),
				Err(e) => Self::deposit_event(Event::ErrorSendingPong(e, para, seq, payload)),
			}
			Ok(())
		}

		#[pallet::call_index(5)]
		#[pallet::weight(0)]
		pub fn pong(origin: OriginFor<T>, seq: u32, payload: Vec<u8>) -> DispatchResult {
			// Only accept pings from other chains.
			let para = ensure_sibling_para(<T as Config>::RuntimeOrigin::from(origin))?;

			if let Some(sent_at) = Pings::<T>::take(seq) {
				Self::deposit_event(Event::Ponged(
					para,
					seq,
					payload,
					frame_system::Pallet::<T>::block_number().saturating_sub(sent_at),
				));
			} else {
				// Pong received for a ping we apparently didn't send?!
				Self::deposit_event(Event::UnknownPong(para, seq, payload));
			}
			Ok(())
		}
	}
}
