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

//! TODO

#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;
extern crate alloc;

use frame_support::pallet_prelude::*;
use sp_runtime::traits::Convert;
use sp_staking::{Exposure, SessionIndex};
use xcm::prelude::*;

/// The balance type of this pallet.
pub type BalanceOf<T> = <T as Config>::CurrencyBalance;

const LOG_TARGET: &str = "runtime::staking::ah-client";

/// `pallet-staking-rc-client` pallet index on AssetHub. Used to construct remote calls.
///
/// The codec index must
/// correspond to the index of `pallet-staking-rc-client` in the `construct_runtime` of AssetHub.
#[derive(Encode, Decode)]
enum AssetHubRuntimePallets {
	#[codec(index = 50)]
	RcClient(StakingCalls),
}

/// Call encoding for the calls needed from the Broker pallet.
#[derive(Encode, Decode)]
enum StakingCalls {
	#[codec(index = 1)]
	StartSession(SessionIndex),
	#[codec(index = 2)]
	EndSession(SessionIndex),
}

#[frame_support::pallet(dev_mode)]
pub mod pallet {
	use crate::*;
	use alloc::vec::Vec;
	use frame_system::pallet_prelude::*;
	use pallet_session::historical;
	use pallet_staking::ExposureOf;
	use sp_runtime::Perbill;
	use sp_staking::{
		offence::{OffenceDetails, OnOffenceHandler},
		SessionIndex,
	};

	const STORAGE_VERSION: StorageVersion = StorageVersion::new(1);
	#[pallet::pallet]
	#[pallet::storage_version(STORAGE_VERSION)]
	pub struct Pallet<T>(_);

	// TODO: should contain some initial state, otherwise starting from genesis won't work
	#[pallet::storage]
	pub type ValidatorSet<T: Config> = StorageValue<
		_,
		Option<Vec<(T::AccountId, Exposure<T::AccountId, BalanceOf<T>>)>>,
		ValueQuery,
	>;

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// Just the `Currency::Balance` type; we have this item to allow us to constrain it to
		/// `From<u64>`.
		type CurrencyBalance: sp_runtime::traits::AtLeast32BitUnsigned
			+ codec::FullCodec
			+ Copy
			+ MaybeSerializeDeserialize
			+ core::fmt::Debug
			+ Default
			+ From<u64>
			+ TypeInfo
			+ Send
			+ Sync
			+ MaxEncodedLen;

		/// The ParaId of the AH-next chain.
		#[pallet::constant]
		type AssetHubId: Get<u32>;
		/// The XCM sender.
		type SendXcm: SendXcm;
		/// Maximum weight for any XCM transact call that should be executed on AssetHub.
		///
		/// Should be `max_weight(set_leases, reserve, notify_core_count)`.
		/// TODO: Update this comment ^^^
		type MaxXcmTransactWeight: Get<Weight>;
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::call_index(0)]
		// #[pallet::weight(T::WeightInfo::new_validators())] // TODO
		pub fn new_validators(
			origin: OriginFor<T>,
			new_validator_set: Vec<(T::AccountId, Exposure<T::AccountId, BalanceOf<T>>)>,
		) -> DispatchResult {
			// TODO: check origin

			// Save the validator set. We don't care if there is a validator set which was not used.
			ValidatorSet::<T>::put(Some(new_validator_set));

			Ok(())
		}
	}

	impl<T: Config> historical::SessionManager<T::AccountId, Exposure<T::AccountId, BalanceOf<T>>>
		for Pallet<T>
	{
		fn new_session(
			new_index: sp_staking::SessionIndex,
		) -> Option<Vec<(T::AccountId, Exposure<T::AccountId, BalanceOf<T>>)>> {
			// todo: check if we need to keep a copy of the validator set in case of another call to
			// `new_session` before we get new validators. My assumption right now is that
			// returning `None` will cause validator set to remain unchanged.
			ValidatorSet::<T>::take()
		}

		// This method is supposed to be used by
		fn new_session_genesis(
			new_index: SessionIndex,
		) -> Option<Vec<(T::AccountId, Exposure<T::AccountId, BalanceOf<T>>)>> {
			ValidatorSet::<T>::take()
		}

		fn start_session(start_index: SessionIndex) {
			<Self as pallet_session::SessionManager<_>>::start_session(start_index)
		}

		fn end_session(end_index: SessionIndex) {
			<Self as pallet_session::SessionManager<_>>::end_session(end_index)
		}
	}

	impl<T: Config> pallet_session::SessionManager<T::AccountId> for Pallet<T> {
		fn new_session(_: u32) -> Option<Vec<<T as frame_system::Config>::AccountId>> {
			// Doesn't do anything because all the logic is handled in `historical::SessionManager`
			// implementation
			defensive!("new_session should not be called");
			None
		}

		fn end_session(session_index: u32) {
			// todo: pass era points info

			let message = Xcm(vec![
				Instruction::UnpaidExecution {
					weight_limit: WeightLimit::Unlimited,
					check_origin: None,
				},
				mk_asset_hub_call::<T>(StakingCalls::EndSession(session_index)),
			]);
			if let Err(err) = send_xcm::<T::SendXcm>(
				Location::new(0, [Junction::Parachain(T::AssetHubId::get())]),
				message,
			) {
				log::error!(target: LOG_TARGET, "Sending `EndSession` to AssetHub failed: {:?}", err);
			}
		}

		fn start_session(session_index: u32) {
			// todo: pass active validator set somehow(tm)

			let message = Xcm(vec![
				Instruction::UnpaidExecution {
					weight_limit: WeightLimit::Unlimited,
					check_origin: None,
				},
				mk_asset_hub_call::<T>(StakingCalls::StartSession(session_index)),
			]);
			if let Err(err) = send_xcm::<T::SendXcm>(
				Location::new(0, [Junction::Parachain(T::AssetHubId::get())]),
				message,
			) {
				log::error!(target: LOG_TARGET, "Sending `StartSession` to AssetHub failed: {:?}", err);
			}
		}
	}

	impl<T> pallet_authorship::EventHandler<T::AccountId, BlockNumberFor<T>> for Pallet<T>
	where
		T: Config + pallet_authorship::Config + pallet_session::Config,
	{
		fn note_author(author: T::AccountId) {
			// save the account id in a storage item
			todo!()
		}
	}

	impl<T: Config>
		OnOffenceHandler<T::AccountId, pallet_session::historical::IdentificationTuple<T>, Weight>
		for Pallet<T>
	where
		T: pallet_session::Config<ValidatorId = <T as frame_system::Config>::AccountId>,
		T: pallet_session::historical::Config<
			FullIdentification = Exposure<<T as frame_system::Config>::AccountId, BalanceOf<T>>,
			FullIdentificationOf = ExposureOf<T>,
		>,
		T::SessionHandler: pallet_session::SessionHandler<<T as frame_system::Config>::AccountId>,
		T::SessionManager: pallet_session::SessionManager<<T as frame_system::Config>::AccountId>,
		T::ValidatorIdOf: Convert<
			<T as frame_system::Config>::AccountId,
			Option<<T as frame_system::Config>::AccountId>,
		>,
	{
		fn on_offence(
			offenders: &[OffenceDetails<
				T::AccountId,
				pallet_session::historical::IdentificationTuple<T>,
			>],
			slash_fraction: &[Perbill],
			slash_session: SessionIndex,
		) -> Weight {
			// send the offender immediately over xcm
			todo!()
		}
	}

	fn mk_asset_hub_call<T: Config>(call: StakingCalls) -> Instruction<()> {
		Instruction::Transact {
			origin_kind: OriginKind::Superuser,
			fallback_max_weight: Some(T::MaxXcmTransactWeight::get()),
			call: AssetHubRuntimePallets::RcClient(call).encode().into(),
		}
	}
}
