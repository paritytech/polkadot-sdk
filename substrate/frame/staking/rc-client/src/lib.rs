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

//! This pallet is intended to be used on AssetHub. It provides extrinsics used by
//! `pallet-staking-ah-client` and serves as an interface between the relay chain and the staking
//! pallet on AssetHub.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use alloc::vec::Vec;
use frame_support::pallet_prelude::*;
use sp_core::crypto::AccountId32;
use sp_runtime::Perbill;
use sp_staking::SessionIndex;
use xcm::prelude::*;

const LOG_TARGET: &str = "runtime::staking::rc-client";

// Provides to the pallet a validator set produced by an election or other similar mechanism.
pub trait ElectionResultHandler<ValidatorId> {
	fn handle_election_result(result: Vec<ValidatorId>);
}

// API provided by the staking pallet.
pub trait StakingApi {
	/// New session with index `start_index` has started on the relay chain.
	fn on_relay_chain_session_start(start_index: SessionIndex);
	/// A session with index `end_index` has ended on the relay chain. The block authors and their
	/// corresponding session points are reported.
	fn on_relay_chain_session_end(end_index: SessionIndex, block_authors: Vec<(AccountId32, u32)>);
	/// Report one or more offences on the relay chain.
	fn on_new_offences(offences: Vec<Offence>);
}

/// `pallet-staking-ah-client` pallet index on Relay chain. Used to construct remote calls.
///
/// The codec index must correspond to the index of `pallet-staking-ah-client` in the
/// `construct_runtime` of the Relay chain.
#[derive(Encode, Decode)]
enum RelayChainRuntimePallets {
	#[codec(index = 50)]
	AhClient(SessionCalls),
}

/// Call encoding for the calls needed from the pallet.
#[derive(Encode, Decode)]
enum SessionCalls {
	#[codec(index = 0)]
	NewValidatorSet(Vec<AccountId32>),
}

// An offence on the relay chain. Based on [`sp_staking::offence::OffenceDetails`].
#[derive(Encode, Decode, Debug, Clone, PartialEq, TypeInfo)]
pub struct Offence {
	offender: AccountId32,
	reporters: Vec<AccountId32>,
	slash_fraction: Perbill,
}

impl Offence {
	pub fn new(
		offender: AccountId32,
		reporters: Vec<AccountId32>,
		slash_fraction: Perbill,
	) -> Self {
		Self { offender, reporters, slash_fraction }
	}
}

#[frame_support::pallet(dev_mode)]
pub mod pallet {
	use super::*;
	use alloc::vec;
	use frame_system::pallet_prelude::*;

	/// The in-code storage version.
	const STORAGE_VERSION: StorageVersion = StorageVersion::new(1);

	#[pallet::pallet]
	#[pallet::storage_version(STORAGE_VERSION)]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		type AdminOrigin: EnsureOrigin<Self::RuntimeOrigin>;
		/// A stable ID for a validator.
		type ValidatorId: Member
			+ Parameter
			+ MaybeSerializeDeserialize
			+ MaxEncodedLen
			+ TryFrom<Self::AccountId>;

		/// Handler for staking calls
		type StakingApi: StakingApi;
		/// The XCM sender.
		type SendXcm: SendXcm;
	}

	impl<T: Config, ValidatorId: Into<AccountId32>> ElectionResultHandler<ValidatorId> for Pallet<T> {
		fn handle_election_result(result: Vec<ValidatorId>) {
			let new_validator_set = result.into_iter().map(Into::into).collect::<Vec<_>>();

			let message = Xcm(vec![
				Instruction::UnpaidExecution {
					weight_limit: WeightLimit::Unlimited,
					check_origin: None,
				},
				mk_relay_chain_call(SessionCalls::NewValidatorSet(new_validator_set)),
			]);

			if let Err(err) = send_xcm::<T::SendXcm>(Location::new(1, Here), message) {
				log::error!(target: LOG_TARGET, "Sending `NewValidators` to relay chain failed: {:?}", err);
			}
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Called to indicate the start of a new session on the relay chain.
		#[pallet::call_index(0)]
		// #[pallet::weight(T::WeightInfo::end_session())] // TODO
		pub fn relay_chain_session_start(
			origin: OriginFor<T>,
			start_index: SessionIndex,
		) -> DispatchResult {
			T::AdminOrigin::ensure_origin_or_root(origin)?;
			T::StakingApi::on_relay_chain_session_start(start_index);
			Ok(())
		}

		/// Called to indicate the end of a session on the relay chain. Accepts the session id and
		/// the block authors with their corresponding session points for the finished session.
		#[pallet::call_index(1)]
		// #[pallet::weight(T::WeightInfo::end_session())] // TODO
		pub fn relay_chain_session_end(
			origin: OriginFor<T>,
			end_index: SessionIndex,
			block_authors: Vec<(AccountId32, u32)>,
		) -> DispatchResult {
			T::AdminOrigin::ensure_origin_or_root(origin)?;
			T::StakingApi::on_relay_chain_session_end(end_index, block_authors);
			Ok(())
		}

		/// Called to report one or more new offenses on the relay chain.
		#[pallet::call_index(2)]
		// #[pallet::weight(T::WeightInfo::end_session())] // TODO
		pub fn new_relay_chain_offence(
			origin: OriginFor<T>,
			offences: Vec<Offence>,
		) -> DispatchResult {
			T::AdminOrigin::ensure_origin_or_root(origin)?;
			T::StakingApi::on_new_offences(offences);
			Ok(())
		}
	}

	fn mk_relay_chain_call(call: SessionCalls) -> Instruction<()> {
		Instruction::Transact {
			origin_kind: OriginKind::Superuser,
			fallback_max_weight: None,
			call: RelayChainRuntimePallets::AhClient(call).encode().into(),
		}
	}
}
