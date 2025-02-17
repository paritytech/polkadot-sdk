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

//! This pallet is intended to be used on a relay chain and to communicate with its counterpart on
//! AssetHub (or a similar network) named `pallet-staking-rc-client`.
//!
//! This pallet serves as an interface between the staking pallet on AssetHub and the session pallet
//! on the relay chain. From the relay chain to AssetHub, its responsibilities are to send
//! information about session changes (start and end) and to report offenses. From AssetHub to the
//! relay chain, it receives information about the potentially new validator set for the session.
//!
//! All the communication between the two pallets is performed with XCM messages.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use alloc::vec::Vec;
use frame_support::pallet_prelude::*;
use pallet_staking_rc_client::Offence;
use sp_core::crypto::AccountId32;
use sp_runtime::traits::Convert;
use sp_staking::{offence::OffenceDetails, Exposure, SessionIndex};
use xcm::prelude::*;

const LOG_TARGET: &str = "runtime::staking::ah-client";

/// `pallet-staking-rc-client` pallet index on AssetHub. Used to construct remote calls.
///
/// The codec index must correspond to the index of `pallet-staking-rc-client` in the
/// `construct_runtime` of AssetHub.
#[derive(Encode, Decode)]
enum AssetHubRuntimePallets {
	#[codec(index = 50)]
	RcClient(StakingCalls),
}

/// Call encoding for the calls needed from the rc-client pallet.
#[derive(Encode, Decode)]
enum StakingCalls {
	/// A session with the given index has started.
	#[codec(index = 0)]
	RelayChainSessionStart(SessionIndex),
	// A session with the given index has ended. The block authors with their corresponding
	// session points are provided.
	#[codec(index = 1)]
	RelayChainSessionEnd(SessionIndex, Vec<(AccountId32, u32)>),
	/// Report one or more offences.
	#[codec(index = 2)]
	NewRelayChainOffences(SessionIndex, Vec<Offence>),
}

#[frame_support::pallet(dev_mode)]
pub mod pallet {
	use crate::*;
	use alloc::vec;
	use core::result;
	use frame_system::pallet_prelude::*;
	use pallet_session::historical;
	use pallet_staking::ExposureOf;
	use polkadot_primitives::Id as ParaId;
	use polkadot_runtime_parachains::origin::{ensure_parachain, Origin};
	use sp_runtime::Perbill;
	use sp_staking::{offence::OnOffenceHandler, SessionIndex};

	const STORAGE_VERSION: StorageVersion = StorageVersion::new(1);

	/// The balance type of this pallet.
	pub type BalanceOf<T> = <T as Config>::CurrencyBalance;

	// `Exposure<T::AccountId, BalanceOf<T>>` will be removed. This type alias exists only to
	// suppress clippy warnings.
	type ElectedValidatorSet<T> = Vec<(
		<T as frame_system::Config>::AccountId,
		Exposure<<T as frame_system::Config>::AccountId, BalanceOf<T>>,
	)>;

	#[pallet::pallet]
	#[pallet::storage_version(STORAGE_VERSION)]
	pub struct Pallet<T>(_);

	// TODO: should contain some initial state, otherwise starting from genesis won't work
	#[pallet::storage]
	pub type ValidatorSet<T: Config> = StorageValue<_, Option<ElectedValidatorSet<T>>, ValueQuery>;

	/// Keeps track of the session points for each block author in the current session.
	#[pallet::storage]
	pub type BlockAuthors<T: Config> = StorageMap<_, Twox64Concat, AccountId32, u32, ValueQuery>;

	#[pallet::config]
	pub trait Config: frame_system::Config {
		type RuntimeOrigin: From<<Self as frame_system::Config>::RuntimeOrigin>
			+ Into<result::Result<Origin, <Self as Config>::RuntimeOrigin>>;
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
		/// The ParaId of the AssetHub.
		#[pallet::constant]
		type AssetHubId: Get<u32>;
		/// The XCM sender.
		type SendXcm: SendXcm;
	}

	#[pallet::error]
	pub enum Error<T> {
		/// The ParaId making the call is not AssetHub.
		NotAssetHub,
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::call_index(0)]
		// #[pallet::weight(T::WeightInfo::new_validators())] // TODO
		pub fn new_validator_set(
			origin: OriginFor<T>,
			new_validator_set: ElectedValidatorSet<T>,
		) -> DispatchResult {
			// Ignore requests not coming from the AssetHub or root.
			Self::ensure_root_or_para(origin, <T as Config>::AssetHubId::get().into())?;

			// Save the validator set. We don't care if there is a validator set which was not used.
			ValidatorSet::<T>::put(Some(new_validator_set));

			Ok(())
		}
	}

	impl<T: Config> historical::SessionManager<T::AccountId, Exposure<T::AccountId, BalanceOf<T>>>
		for Pallet<T>
	{
		fn new_session(_: sp_staking::SessionIndex) -> Option<ElectedValidatorSet<T>> {
			// If there is a new validator set - return it. Otherwise return `None`.
			ValidatorSet::<T>::take()
		}

		fn new_session_genesis(
			_: SessionIndex,
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
			let authors = BlockAuthors::<T>::iter().collect::<Vec<_>>();
			// The maximum number of block authors is `num_cores * max_validators_per_core` (both
			// are parameters from [`SchedulerParams`]).
			let _ = BlockAuthors::<T>::clear(u32::MAX, None);

			let message = Xcm(vec![
				Instruction::UnpaidExecution {
					weight_limit: WeightLimit::Unlimited,
					check_origin: None,
				},
				mk_asset_hub_call(StakingCalls::RelayChainSessionEnd(session_index, authors)),
			]);

			if let Err(err) = send_xcm::<T::SendXcm>(
				Location::new(0, [Junction::Parachain(T::AssetHubId::get())]),
				message,
			) {
				log::error!(target: LOG_TARGET, "Sending `RelayChainSessionEnd` to AssetHub failed: {:?}", err);
			}
		}

		fn start_session(session_index: u32) {
			let message = Xcm(vec![
				Instruction::UnpaidExecution {
					weight_limit: WeightLimit::Unlimited,
					check_origin: None,
				},
				mk_asset_hub_call(StakingCalls::RelayChainSessionStart(session_index)),
			]);
			if let Err(err) = send_xcm::<T::SendXcm>(
				Location::new(0, [Junction::Parachain(T::AssetHubId::get())]),
				message,
			) {
				log::error!(target: LOG_TARGET, "Sending `RelayChainSessionStart` to AssetHub failed: {:?}", err);
			}
		}
	}

	impl<T> pallet_authorship::EventHandler<T::AccountId, BlockNumberFor<T>> for Pallet<T>
	where
		T: Config + pallet_authorship::Config + pallet_session::Config + Config,
		T::AccountId: Into<AccountId32>,
	{
		// Notes the authored block in `BlockAuthors`.
		fn note_author(author: T::AccountId) {
			BlockAuthors::<T>::mutate(author.into(), |block_count| {
				*block_count += 1;
			});
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
		T::AccountId: Into<AccountId32>,
	{
		fn on_offence(
			offenders: &[OffenceDetails<
				T::AccountId,
				pallet_session::historical::IdentificationTuple<T>,
			>],
			slash_fraction: &[Perbill],
			slash_session: SessionIndex,
		) -> Weight {
			let offenders_and_slashes = offenders
				.iter()
				.cloned()
				.zip(slash_fraction)
				.map(|(offence, fraction)| {
					Offence::new(
						offence.offender.0.into(),
						offence.reporters.into_iter().map(|r| r.into()).collect(),
						*fraction,
					)
				})
				.collect::<Vec<_>>();

			// send the offender immediately over xcm
			let message = Xcm(vec![
				Instruction::UnpaidExecution {
					weight_limit: WeightLimit::Unlimited,
					check_origin: None,
				},
				mk_asset_hub_call(StakingCalls::NewRelayChainOffences(
					slash_session,
					offenders_and_slashes,
				)),
			]);
			if let Err(err) = send_xcm::<T::SendXcm>(
				Location::new(0, [Junction::Parachain(T::AssetHubId::get())]),
				message,
			) {
				log::error!(target: LOG_TARGET, "Sending `NewRelayChainOffences` to AssetHub failed: {:?}",
			err);
			}

			Weight::zero()
		}
	}

	impl<T: Config> Pallet<T> {
		/// Ensure the origin is one of Root or the `para` itself.
		fn ensure_root_or_para(
			origin: <T as frame_system::Config>::RuntimeOrigin,
			id: ParaId,
		) -> DispatchResult {
			if let Ok(caller_id) =
				ensure_parachain(<T as Config>::RuntimeOrigin::from(origin.clone()))
			{
				// Check if matching para id...
				ensure!(caller_id == id, Error::<T>::NotAssetHub);
			} else {
				// Check if root...
				ensure_root(origin.clone())?;
			}
			Ok(())
		}
	}

	fn mk_asset_hub_call(call: StakingCalls) -> Instruction<()> {
		Instruction::Transact {
			origin_kind: OriginKind::Superuser,
			fallback_max_weight: None,
			call: AssetHubRuntimePallets::RcClient(call).encode().into(),
		}
	}
}
