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

use frame_support::pallet_prelude::EnsureOrigin;
pub use pallet::*;

#[frame_support::pallet(dev_mode)]
pub mod pallet {
	use frame_support::pallet_prelude::EnsureOrigin;
	use frame_support::{
		dispatch::{DispatchResult, Pays, PostDispatchInfo},
		ensure,
		pallet_prelude::DispatchResultWithPostInfo,
		weights::Weight,
	};
	use frame_system::pallet_prelude::*;


	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		type ContractOrigin: EnsureOrigin<Self::RuntimeOrigin>;
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Dummy function that overcharges the predispatch weight, allowing us to test the correct
		/// values of [`ContractResult::gas_consumed`] and [`ContractResult::gas_required`] in
		/// tests.
		#[pallet::call_index(1)]
		#[pallet::weight(*pre_charge)]
		pub fn overestimate_pre_charge(
			origin: OriginFor<T>,
			pre_charge: Weight,
			actual_weight: Weight,
		) -> DispatchResultWithPostInfo {
			ensure_signed(origin)?;
			ensure!(pre_charge.any_gt(actual_weight), "pre_charge must be > actual_weight");
			Ok(PostDispatchInfo { actual_weight: Some(actual_weight), pays_fee: Pays::Yes })
		}

		#[pallet::call_index(2)]
		#[pallet::weight(Weight::zero())]
		pub fn contract_only(origin: OriginFor<T>) -> DispatchResult {
			T::ContractOrigin::ensure_origin(origin)?;
			Ok(())
		}
	}
}


use core::marker::PhantomData;
use crate::ContractOrigin;

pub struct EnsureContract<AccountId>(
	PhantomData<AccountId >,
);
impl<
		O: Into<Result<ContractOrigin<AccountId>, O>> + From<ContractOrigin<AccountId>>,
		AccountId,
	> EnsureOrigin<O> for EnsureContract<AccountId>
{
	type Success = AccountId;
	fn try_origin(o: O) -> Result<Self::Success, O> {
		o.into().and_then(|o| match o {
			ContractOrigin::Signed(id) => Ok(id),
			r => Err(O::from(r)),
		})
	}

}


