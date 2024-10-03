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

//! Types & Imports for Distribution pallet.

pub use super::*;

pub use frame_support::{
	pallet_prelude::*,
	traits::{
		fungible,
		fungible::{Inspect, Mutate, MutateHold},
		fungibles,
		tokens::{Precision, Preservation},
		DefensiveOption, EnsureOrigin,
	},
	transactional, PalletId, Serialize,
};
pub use frame_system::{pallet_prelude::*, RawOrigin};
pub use scale_info::prelude::vec::Vec;
pub use sp_runtime::traits::{
	AccountIdConversion, BlockNumberProvider, Convert, Saturating, StaticLookup, Zero,
};
pub use weights::WeightInfo;

pub type BalanceOf<T> = <<T as Config>::NativeBalance as fungible::Inspect<
	<T as frame_system::Config>::AccountId,
>>::Balance;
pub type AccountIdOf<T> = <T as frame_system::Config>::AccountId;
/// A reward index.
pub type SpendIndex = u32;

pub type ProjectId<T> = AccountIdOf<T>;

/// The state of the payment claim.
#[derive(Encode, Decode, Clone, PartialEq, Eq, MaxEncodedLen, RuntimeDebug, TypeInfo, Default)]
pub enum SpendState {
	/// Unclaimed
	#[default]
	Unclaimed,
	/// Claimed & Paid.
	Completed,
	/// Claimed but Failed.
	Failed,
}

//Processed Reward status
#[derive(Encode, Decode, Clone, PartialEq, MaxEncodedLen, RuntimeDebug, TypeInfo)]
#[scale_info(skip_type_params(T))]
pub struct SpendInfo<T: Config> {
	/// The asset amount of the spend.
	pub amount: BalanceOf<T>,
	/// The block number from which the spend can be claimed(24h after SpendStatus Creation).
	pub valid_from: BlockNumberFor<T>,
	/// Corresponding project id
	pub whitelisted_project: Option<AccountIdOf<T>>,
	/// Has it been claimed?
	pub claimed: bool,
}

impl<T: Config> SpendInfo<T> {
	pub fn new(whitelisted: &ProjectInfo<T>) -> Self {
		let amount = whitelisted.amount;
		let whitelisted_project = Some(whitelisted.project_id.clone());
		let claimed = false;
		let valid_from =
			<frame_system::Pallet<T>>::block_number().saturating_add(T::BufferPeriod::get());

		let spend = SpendInfo { amount, valid_from, whitelisted_project, claimed };

		//Add it to the Spends storage
		Spends::<T>::insert(whitelisted.project_id.clone(), spend.clone());

		spend
	}
}

#[derive(Encode, Decode, Clone, PartialEq, Eq, MaxEncodedLen, RuntimeDebug, TypeInfo)]
#[scale_info(skip_type_params(T))]
pub struct ProjectInfo<T: Config> {
	/// AcountId that will receive the payment.
	pub project_id: ProjectId<T>,

	/// Block at which the project was submitted for reward distribution
	pub submission_block: BlockNumberFor<T>,

	/// Amount to be lock & pay for this project
	pub amount: BalanceOf<T>,
}
